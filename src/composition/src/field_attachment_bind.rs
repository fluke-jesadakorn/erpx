//! Files record binding composed with Field, Identity, Commercial and Workflow owners.
use crate::field_attachment::{
    AttachmentContext, FieldAttachmentBindResponse, FieldAttachmentBindResult,
    FieldAttachmentManifestItem,
};
use erpx_server_files::public::field_visit_bind::{
    CurrentFieldBindSet, FieldBindManifestItem, prepare, read_field_bind_set,
    verify_field_bind_replay,
};
use erpx_server_identity_access::public::{
    command_metadata::project_command_iam,
    effective_authority::{AuthorizedWorkContext, HoldingWorkSourcePreparation},
    work_context::WorkScopeKind,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, IdentifierSource, RecordId};
use erpx_server_workflow::public::commands::*;
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ATTACHMENT_SET_CHANGED",
        "The original file set changed.",
    )
}
#[derive(Clone)]
pub(crate) struct BindSources;
impl HoldingWorkSourcePreparation for BindSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            crate::field_attachment::lock_attachment_sources(c, true).await?;
            erpx_server_workflow::public::record_origin::lock_record_origin_sources(c).await?;
            Ok(())
        })
    }
}
fn actor(current: &AuthorizedWorkContext, now: i64) -> Result<CommandActor, BusinessError> {
    current.require_permission("records.read")?;
    if now >= current.expires_at() {
        return Err(changed());
    }
    let s = current.selection();
    let w = s.workspace();
    let a = s.assignment();
    Ok(CommandActor {
        user_id: current.user_id().clone(),
        assignment_id: a.id.clone(),
        role: a.role.clone(),
        company_id: w.company_id.clone(),
        branch_id: w
            .branch_id
            .as_ref()
            .map(|v| CommandField::Value(v.as_str().into()))
            .unwrap_or(CommandField::Null),
        department_id: w
            .department_id
            .as_ref()
            .map(|v| CommandField::Value(v.as_str().into()))
            .unwrap_or(CommandField::Null),
        authority: CommandField::Value(WorkAuthority {
            session_id: current.session_id().as_str().into(),
            revision: s.revision(),
            assignment_version: a.version,
        }),
        iam: CommandField::Value(CommandJson::from_serializable(&project_command_iam(
            current,
        )?)?),
        assignment_scope_kind: match a.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .into(),
        assignment_scope_node_id: a.scope_node_id.as_ref().map(|v| v.as_str().into()),
    })
}
struct Authority<'a, C> {
    ctx: &'a AttachmentContext<'a, C>,
    request: &'a WorkflowRequest,
    source: Option<crate::field_attachment::Proof>,
    replay: bool,
}
impl<C: Clock + Sync> PgCommandAuthority for Authority<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        BindSources.prepare(c).await
    }
    async fn authorize(
        &mut self,
        _: &mut PgConnection,
        r: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        if r.key != self.request.key
            || r.record_id != self.request.record_id
            || r.operation != self.request.operation
            || r.input.as_str() != self.request.input.as_str()
        {
            return Err(changed().into());
        }
        Ok(actor(self.ctx.actor, self.ctx.clock.now().get())?)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        a: &CommandActor,
        r: &WorkflowRequest,
        disposition: CommandDisposition,
    ) -> Result<(), Error> {
        if actor(self.ctx.actor, self.ctx.clock.now().get())? != *a {
            return Err(changed().into());
        }
        let replay = disposition == CommandDisposition::Replay;
        if !replay {
            self.ctx.actor.require_permission("records.write")?
        }
        if self.source.is_none() || self.replay != replay {
            self.source =
                Some(crate::field_attachment::proof(c, self.ctx, &r.record_id, !replay).await?);
            self.replay = replay
        }
        self.source
            .as_ref()
            .ok_or_else(changed)?
            .current_file_bind(
                c,
                self.ctx.actor,
                self.ctx.holding,
                self.ctx.company,
                self.ctx.catalogue,
                self.ctx.clock,
            )
            .await
    }
}
struct Prepared {
    set: CurrentFieldBindSet,
    proof: crate::field_attachment::Proof,
    manifest: Vec<FieldBindManifestItem>,
}
struct Effects<'a, C> {
    ctx: &'a AttachmentContext<'a, C>,
    set_id: &'a str,
    expected: i64,
    prepared: Option<Prepared>,
    written: Option<Prepared>,
}
fn manifest(items: &[FieldBindManifestItem]) -> Vec<FieldAttachmentManifestItem> {
    items
        .iter()
        .map(|v| FieldAttachmentManifestItem {
            link_id: v.link_id.clone(),
            source_id: v.source_id.clone(),
            purpose_key: v.purpose_key.clone(),
            checksum_sha256: v.checksum_sha256.clone(),
            media_type: v.media_type.clone(),
            bytes: v.bytes,
        })
        .collect()
}
impl<C: Clock + Sync> PgOwnerCommandEffects for Effects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        BindSources.prepare(c).await
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let set = read_field_bind_set(c, self.ctx.company, self.set_id).await?;
        let data = set.data();
        let items = prepare(data, self.expected)?;
        let p = crate::field_attachment::proof(c, self.ctx, &data.target_id, true).await?;
        if p.field.facts().version != self.expected {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "ATTACHMENT_TARGET_CHANGED",
                "The record changed before evidence was bound.",
            )
            .into());
        }
        let version = self
            .expected
            .checked_add(1)
            .filter(|v| *v <= 9_007_199_254_740_991)
            .ok_or_else(changed)?;
        let result = FieldAttachmentBindResult {
            set_id: self.set_id.into(),
            version,
            manifest: manifest(&items),
        };
        let json = serde_json::to_value(&result).map_err(|_| changed())?;
        if !crate::field_service_workspace::safe_json(&json) {
            return Err(changed().into());
        }
        serde_json::to_writer(crate::field_service_reservation_read::ReplyBudget(0), &json)
            .map_err(|_| changed())?;
        let scope = CommandEventScope {
            company_id: self.ctx.company.clone(),
            branch_id: data
                .branch_id
                .clone()
                .map(CommandField::Value)
                .unwrap_or(CommandField::Null),
            department_id: data
                .department_id
                .clone()
                .map(CommandField::Value)
                .unwrap_or(CommandField::Null),
        };
        self.prepared = Some(Prepared {
            set,
            proof: p,
            manifest: items,
        });
        Ok(WorkflowOutput {
            version,
            result: CommandJson::from_serializable(&result)?,
            event_scope: Some(scope),
        })
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        r: &WorkflowRequest,
    ) -> Result<(), Error> {
        let p = self.prepared.take().ok_or_else(changed)?;
        p.proof
            .current_file_bind(
                c,
                self.ctx.actor,
                self.ctx.holding,
                self.ctx.company,
                self.ctx.catalogue,
                self.ctx.clock,
            )
            .await?;
        p.set.require_current(c).await?;
        erpx_server_field_service::public::file_source::bind_field_file_evidence(
            c,
            self.ctx.actor,
            &p.proof.field,
            &r.key,
            self.expected,
            self.ctx.clock.now().get(),
        )
        .await?;
        p.set
            .apply(
                c,
                &p.manifest,
                self.ctx.actor.user_id().as_str(),
                &r.key,
                self.ctx.clock.now().get(),
            )
            .await?;
        self.written = Some(p);
        Ok(())
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        r: &WorkflowRequest,
    ) -> Result<(), Error> {
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *c)
            .await?;
        let p = self.written.as_ref().ok_or_else(changed)?;
        p.proof
            .current_file_bind(
                c,
                self.ctx.actor,
                self.ctx.holding,
                self.ctx.company,
                self.ctx.catalogue,
                self.ctx.clock,
            )
            .await?;
        let after = read_field_bind_set(c, self.ctx.company, self.set_id).await?;
        verify_field_bind_replay(
            c,
            &after,
            &r.key,
            self.ctx.actor.user_id().as_str(),
            &p.manifest,
        )
        .await?;
        erpx_server_field_service::public::file_source::verify_field_file_bind(
            c,
            self.ctx.actor,
            &p.proof.field,
            &r.key,
            self.expected + 1,
        )
        .await
    }
}
struct Ids;
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("generated UUID")
    }
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReplayResult {
    set_id: String,
    version: i64,
    manifest: Vec<FieldBindManifestItem>,
}
pub(crate) async fn bind<C: Clock + Sync>(
    c: &mut PgConnection,
    ctx: &AttachmentContext<'_, C>,
    set_id: &str,
    key: &str,
    expected: i64,
) -> Result<FieldAttachmentBindResponse, Error> {
    let set = read_field_bind_set(c, ctx.company, set_id).await?;
    if set.data().expected_version != expected {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "ATTACHMENT_SET_STALE",
            "Create a record-document set for the current record version.",
        )
        .into());
    }
    let target = set.data().target_id.clone();
    let identity = WorkflowRecordIdentity::from_utf16(target.encode_utf16().collect());
    let raw = format!(
        "{{\"setId\":{},\"expectedTargetVersion\":{expected}}}",
        serde_json::to_string(set_id).map_err(|_| changed())?
    );
    let request = WorkflowRequest {
        key: key.into(),
        operation: "attachment.record.bind".into(),
        record_id: target.clone(),
        input: CommandJson::parse(&raw)?,
    };
    let ordered = SourceOrderedWorkflowRequest {
        key: key.into(),
        operation: request.operation.clone(),
        record_id: identity,
        input: request.input.clone(),
    };
    let mut authority = Authority {
        ctx,
        request: &request,
        source: None,
        replay: false,
    };
    let mut effects = Effects {
        ctx,
        set_id,
        expected,
        prepared: None,
        written: None,
    };
    let mut ids = Ids;
    let outcome = if let Some(r) =
        PgCommandLedger::new(ctx.company.clone(), c, &mut authority, ctx.clock, &mut ids)
            .replay_source_ordered(&ordered, &mut effects)
            .await?
    {
        r
    } else {
        PgCommandLedger::new(ctx.company.clone(), c, &mut authority, ctx.clock, &mut ids)
            .execute_source_ordered(&ordered, &mut effects)
            .await?
    };
    let result: ReplayResult =
        serde_json::from_str(outcome.result.as_str()).map_err(|_| changed())?;
    if result.set_id != set_id || result.version != expected.checked_add(1).ok_or_else(changed)? {
        return Err(changed().into());
    }
    if outcome.replayed {
        let current = read_field_bind_set(c, ctx.company, set_id).await?;
        verify_field_bind_replay(
            c,
            &current,
            key,
            ctx.actor.user_id().as_str(),
            &result.manifest,
        )
        .await?;
        let proof = crate::field_attachment::proof(c, ctx, &target, false).await?;
        erpx_server_field_service::public::file_source::verify_field_file_bind(
            c,
            ctx.actor,
            &proof.field,
            key,
            result.version,
        )
        .await?;
        proof
            .current_file_bind(
                c,
                ctx.actor,
                ctx.holding,
                ctx.company,
                ctx.catalogue,
                ctx.clock,
            )
            .await?;
    }
    Ok(FieldAttachmentBindResponse {
        data: FieldAttachmentBindResult {
            set_id: result.set_id,
            version: result.version,
            manifest: manifest(&result.manifest),
        },
        context: crate::field_service_reservation_read::context(ctx.actor)?,
    })
}
pub(crate) fn normalize_version(bytes: &[u8]) -> Result<i64, BusinessError> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let doc = CommandJson::parse(text).map_err(|_| {
        BusinessError::invalid("VALIDATION_FAILED", "Invalid record document bind.")
    })?;
    let value = doc.property("expectedTargetVersion")?.ok_or_else(|| {
        BusinessError::invalid("VALIDATION_FAILED", "Invalid record document bind.")
    })?;
    let number = value.as_str().parse::<f64>().map_err(|_| {
        BusinessError::invalid("VALIDATION_FAILED", "Invalid record document bind.")
    })?;
    if !number.is_finite()
        || number.fract() != 0.0
        || !(1.0..=9_007_199_254_740_991.0).contains(&number)
    {
        return Err(BusinessError::invalid(
            "VALIDATION_FAILED",
            "Invalid record document bind.",
        ));
    }
    Ok(number as i64)
}
