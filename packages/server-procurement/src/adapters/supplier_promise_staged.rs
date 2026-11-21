//! Unmounted staged native supplier-promise prerequisite. This does not
//! implement legacy TS receipt compatibility, complete evidence parity, or
//! purchase policy. The evidence stage is unmounted and has extra lock behavior.
use crate::adapters::supplier_promise_command::PgSupplierPromiseCommand;
use crate::application::supplier_promise_command::{
    SupplierPromiseCommandRepository, SupplierPromiseCommitPlan, replay,
};
use crate::domain::supplier_promise_command::{
    OPERATION, PreparedSupplierPromise, RecordSupplierPromiseCommand, SupplierPromiseAuthority,
    SupplierPromiseCommandActor, SupplierPromiseRecordReceipt, ValidatedSupplierPromise,
    fingerprint, idempotency_conflict, invalid_receipt, not_found, prepare_supplier_promise,
    promise_id, source_changed, validate_readback,
};
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CommandKey, RecordId, TimestampMillis};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("PROCUREMENT_PROMISE_SERIALIZABLE_REQUIRED").into());
    }
    let held:i64=sqlx::query_scalar("SELECT count(DISTINCT relation) FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND relation IN ('public.commerce_orders'::regclass,'public.procurement_delivery_promises'::regclass,'public.procurement_delivery_promise_lines'::regclass)").fetch_one(&mut *c).await?;
    if held != 3 {
        return Err(PersistenceError::new("PROCUREMENT_PROMISE_SOURCE_LOCKS_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn actor(current: &AuthorizedWorkContext) -> SupplierPromiseCommandActor {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    SupplierPromiseCommandActor {
        user_id: current.user_id().as_str().into(),
        assignment_id: assignment.id.as_str().into(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        revision: selected.revision(),
        assignment_version: assignment.version,
        permissions: current.command_iam_metadata().permissions.clone(),
        modules: current.modules().to_vec(),
        expires_at: current.expires_at(),
    }
}
fn same(
    current: &AuthorizedWorkContext,
    prior: &SupplierPromiseCommandActor,
    now: TimestampMillis,
) -> Result<(), Error> {
    let actual = actor(current);
    actual.require_record(now)?;
    if actual.user_id != prior.user_id
        || actual.assignment_id != prior.assignment_id
        || actual.company_id != prior.company_id
        || actual.branch_id != prior.branch_id
        || actual.department_id != prior.department_id
        || actual.revision != prior.revision
        || actual.assignment_version != prior.assignment_version
        || actual.expires_at != prior.expires_at
        || actual.permissions != prior.permissions
        || actual.modules != prior.modules
    {
        return Err(source_changed().into());
    }
    Ok(())
}
pub enum SupplierPromiseStart {
    Replay(SupplierPromiseRecordReceipt),
    Fresh(PreparedSupplierPromiseStage),
}
pub struct PreparedSupplierPromiseStage {
    unit: Unit,
    actor: SupplierPromiseCommandActor,
    prepared: PreparedSupplierPromise,
    plan: SupplierPromiseCommitPlan,
    fingerprint: String,
    evidence_sets: Vec<String>,
}
pub struct InsertedSupplierPromiseStage {
    inner: PreparedSupplierPromiseStage,
}
/// Native-only replay before any fresh Commerce source read. This is not the
/// TypeScript historical receipt or schema compatibility boundary.
pub enum SupplierPromiseBegin {
    Replay(SupplierPromiseRecordReceipt),
    Fresh(PendingSupplierPromiseStage),
}
pub struct PendingSupplierPromiseStage {
    unit: Unit,
    actor: SupplierPromiseCommandActor,
    validated: ValidatedSupplierPromise,
    key: CommandKey,
    order: String,
    expected_version: i64,
    fingerprint: String,
    now: TimestampMillis,
    evidence_sets: Vec<String>,
}
pub async fn begin_supplier_promise_stage(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    order_id: &str,
    input: &RecordSupplierPromiseCommand,
    key: &str,
    now: TimestampMillis,
) -> Result<SupplierPromiseBegin, Error> {
    begin(c, current, order_id, input, key, now, false).await
}

/// Experimental native-only evidence stage. The legacy wrapper remains unchanged.
pub async fn begin_supplier_promise_evidence_stage(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    order_id: &str,
    input: &RecordSupplierPromiseCommand,
    key: &str,
    now: TimestampMillis,
) -> Result<SupplierPromiseBegin, Error> {
    begin(c, current, order_id, input, key, now, true).await
}

async fn begin(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    order_id: &str,
    input: &RecordSupplierPromiseCommand,
    key: &str,
    now: TimestampMillis,
    allow_evidence: bool,
) -> Result<SupplierPromiseBegin, Error> {
    let unit = unit(c).await?;
    let actor = actor(current);
    actor.require_record(now)?;
    let validated = input.validate_for_stage(allow_evidence)?;
    let key = CommandKey::new(key)?;
    let order = RecordId::new(order_id)
        .map_err(|_| not_found())?
        .as_str()
        .to_owned();
    let digest = fingerprint(&actor, &order, input)?;
    let mut repository = PgSupplierPromiseCommand::new(&mut *c);
    if let Some(result) = replay(
        repository.receipt(key.as_str()).await?,
        &actor,
        &order,
        &digest,
    )? {
        return Ok(SupplierPromiseBegin::Replay(result));
    }
    Ok(SupplierPromiseBegin::Fresh(PendingSupplierPromiseStage {
        unit,
        actor,
        validated,
        key,
        order,
        expected_version: input.expected_version,
        fingerprint: digest,
        now,
        evidence_sets: input.attachment_set_ids.clone(),
    }))
}
impl PendingSupplierPromiseStage {
    pub async fn prepare(
        self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        now: TimestampMillis,
    ) -> Result<PreparedSupplierPromiseStage, Error> {
        if unit(c).await? != self.unit {
            return Err(source_changed().into());
        }
        same(current, &self.actor, now)?;
        let mut repository = PgSupplierPromiseCommand::new(&mut *c);
        let source = repository
            .source(&self.actor, &self.order)
            .await?
            .ok_or_else(not_found)?;
        self.build(source)
    }
    fn build(
        self,
        source: crate::domain::supplier_promise_command::PromiseSource,
    ) -> Result<PreparedSupplierPromiseStage, Error> {
        let prepared = prepare_supplier_promise(&self.actor, &self.validated, source)?;
        if prepared.order.version != self.expected_version {
            return Err(source_changed().into());
        }
        let plan = SupplierPromiseCommitPlan {
            key: self.key.clone(),
            operation: OPERATION.into(),
            company_id: self.actor.company_id.clone(),
            order_id: self.order,
            actor_id: self.actor.user_id.clone(),
            assignment_id: self.actor.assignment_id.clone(),
            branch_id: self.actor.branch_id.as_ref().map(|v| v.as_str().into()),
            department_id: self.actor.department_id.as_ref().map(|v| v.as_str().into()),
            promise_id: promise_id(&self.key),
            order_version: self.expected_version,
            kind: self.validated.kind.clone(),
            supplier_id: prepared.order.supplier_id.clone(),
            currency: prepared.order.currency.clone(),
            required_date: prepared.order.due_date.clone(),
            confirmation_date: self.validated.confirmation_date.clone(),
            supplier_reference: self.validated.supplier_reference.clone(),
            evidence: self.validated.evidence.clone(),
            lines: prepared.lines.clone(),
            now: self.now,
        };
        Ok(PreparedSupplierPromiseStage {
            unit: self.unit,
            actor: self.actor,
            prepared,
            plan,
            fingerprint: self.fingerprint,
            evidence_sets: self.evidence_sets,
        })
    }
}

/// Enabled-only native prerequisite, not a TS schema or mode parity claim.
/// The Commerce proof has no IAM/purchase authority. The caller must acquire
/// real current Account and purchase authority and apply Files guards first.
pub struct PreparedEnabledSupplierPromise {
    stage: PreparedSupplierPromiseStage,
    source: erpx_server_commerce::public::supplier_promise_source::CurrentSupplierPromiseSource,
}
impl PendingSupplierPromiseStage {
    pub async fn prepare_enabled(
        self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        source: erpx_server_commerce::public::supplier_promise_source::CurrentSupplierPromiseSource,
        now: TimestampMillis,
    ) -> Result<PreparedEnabledSupplierPromise, Error> {
        use crate::domain::supplier_promise_command::{
            PromiseOrderLineSource, PromiseOrderSource, PromiseSource,
        };
        use erpx_server_commerce::public::supplier_promise_source::SupplierPromiseSourceMode;
        if unit(c).await? != self.unit {
            return Err(source_changed().into());
        }
        same(current, &self.actor, now)?;
        if source.mode() != SupplierPromiseSourceMode::Enabled {
            return Err(source_changed().into());
        }
        source.require_same_unit(c).await?;
        let o = source.order();
        if o.id != self.order
            || o.company_id != self.actor.company_id.as_str()
            || self
                .actor
                .branch_id
                .as_ref()
                .is_some_and(|b| b.as_str() != o.branch_id)
            || self
                .actor
                .department_id
                .as_ref()
                .is_some_and(|d| Some(d.as_str()) != o.department_id.as_deref())
            || o.kind != "purchase"
        {
            return Err(not_found().into());
        }
        let order = PromiseOrderSource {
            id: o.id.clone(),
            company_id: o.company_id.clone(),
            branch_id: o.branch_id.clone(),
            department_id: o.department_id.clone(),
            kind: o.kind.clone(),
            state: o.state.clone(),
            version: o.version,
            order_date: o.order_date.clone(),
            due_date: o.due_date.clone(),
            currency: o.currency.clone(),
            supplier_id: o.party_id.clone(),
        };
        let lines = source
            .lines()
            .iter()
            .map(|v| PromiseOrderLineSource {
                id: v.id.clone(),
                quantity_milli: v.quantity_milli,
                fulfilled_milli: v.fulfilled_milli,
            })
            .collect();
        let stage = self.build(PromiseSource { order, lines })?;
        Ok(PreparedEnabledSupplierPromise { stage, source })
    }
}
impl PreparedEnabledSupplierPromise {
    pub fn promise_id(&self) -> &str {
        self.stage.promise_id()
    }
    /// Invoke only after Files staged guards. This performs the TS-shaped
    /// Commerce source guard immediately before the Procurement insert.
    pub async fn insert_after_source_guard(
        self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        now: TimestampMillis,
    ) -> Result<InsertedSupplierPromiseStage, Error> {
        self.source.require_source_guard(c).await?;
        self.stage.insert(c, current, now).await
    }
}

/// Compatibility wrapper retaining the existing native-only behavior.
pub async fn prepare_supplier_promise_stage(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    order: &str,
    input: &RecordSupplierPromiseCommand,
    key: &str,
    now: TimestampMillis,
) -> Result<SupplierPromiseStart, Error> {
    match begin_supplier_promise_stage(c, current, order, input, key, now).await? {
        SupplierPromiseBegin::Replay(value) => Ok(SupplierPromiseStart::Replay(value)),
        SupplierPromiseBegin::Fresh(pending) => Ok(SupplierPromiseStart::Fresh(
            pending.prepare(c, current, now).await?,
        )),
    }
}
impl PreparedSupplierPromiseStage {
    pub fn promise_id(&self) -> &str {
        &self.plan.promise_id
    }
    pub async fn insert(
        self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        now: TimestampMillis,
    ) -> Result<InsertedSupplierPromiseStage, Error> {
        if unit(c).await? != self.unit {
            return Err(source_changed().into());
        }
        same(current, &self.actor, now)?;
        PgSupplierPromiseCommand::new(c)
            .insert_promise(&self.plan)
            .await?;
        Ok(InsertedSupplierPromiseStage { inner: self })
    }
}
impl InsertedSupplierPromiseStage {
    /// Call only after the real Commerce-owned version advancement in this unit.
    pub async fn finalize(
        self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        commerce: &erpx_server_commerce::public::supplier_promise_version::RetainedSupplierPromiseOrder,
        now: TimestampMillis,
    ) -> Result<SupplierPromiseRecordReceipt, Error> {
        if !self.inner.evidence_sets.is_empty() {
            return Err(source_changed().into());
        }
        self.finalize_inner(c, current, commerce, now).await
    }

    /// Requires actual Files effects in this unit; the proof does not grant IAM or purchase authority.
    pub async fn finalize_with_evidence(
        self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        commerce: &erpx_server_commerce::public::supplier_promise_version::RetainedSupplierPromiseOrder,
        evidence: &erpx_server_files::public::supplier_promise_binding::RetainedSupplierPromiseEvidence,
        now: TimestampMillis,
    ) -> Result<SupplierPromiseRecordReceipt, Error> {
        let p = &self.inner;
        if p.evidence_sets.is_empty() {
            return Err(source_changed().into());
        }
        let facts = erpx_server_files::public::supplier_promise_binding::SupplierPromiseEvidenceFacts {
            company_id: p.plan.company_id.as_str().into(),
            order_id: p.plan.order_id.clone(),
            order_version: p.plan.order_version,
            actor_id: p.plan.actor_id.clone(),
            command_key: p.plan.key.as_str().into(),
            promise_id: p.plan.promise_id.clone(),
        };
        evidence.require_applied(c, &facts, &p.evidence_sets).await.map_err(|_| source_changed())?;
        self.finalize_inner(c, current, commerce, now).await
    }

    async fn finalize_inner(
        self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        commerce: &erpx_server_commerce::public::supplier_promise_version::RetainedSupplierPromiseOrder,
        now: TimestampMillis,
    ) -> Result<SupplierPromiseRecordReceipt, Error> {
        let p = self.inner;
        if unit(c).await? != p.unit {
            return Err(source_changed().into());
        }
        same(current, &p.actor, now)?;
        if commerce.company() != p.plan.company_id.as_str()
            || commerce.order_id() != p.plan.order_id
            || commerce.version()
                != p.plan
                    .order_version
                    .checked_add(1)
                    .ok_or_else(source_changed)?
        {
            return Err(source_changed().into());
        }
        commerce.require_current(c).await?;
        let receipt = SupplierPromiseRecordReceipt {
            id: p.plan.order_id.clone(),
            promise_id: p.plan.promise_id.clone(),
            version: p
                .plan
                .order_version
                .checked_add(1)
                .ok_or_else(source_changed)?,
        };
        let result = serde_json::to_string(&receipt).map_err(|_| invalid_receipt())?;
        let authority = serde_json::to_string(&SupplierPromiseAuthority::from_actor(&p.actor))
            .map_err(|_| invalid_receipt())?;
        let mut r = PgSupplierPromiseCommand::new(c);
        let read = r
            .readback(&p.actor.company_id, &p.plan.order_id, &p.plan.promise_id)
            .await?
            .ok_or_else(invalid_receipt)?;
        validate_readback(&read, &p.prepared, &p.plan.promise_id, &p.actor, p.plan.now)?;
        if !r
            .insert_receipt(
                p.plan.key.as_str(),
                OPERATION,
                &p.actor,
                &p.fingerprint,
                &p.plan.order_id,
                &result,
                &authority,
                p.plan.now,
            )
            .await?
        {
            return Err(idempotency_conflict().into());
        }
        r.audit(&p.plan, &receipt, &authority).await?;
        r.outbox(&p.plan, &receipt, &authority).await?;
        Ok(receipt)
    }
}
