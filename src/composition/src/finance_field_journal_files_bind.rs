//! Atomic Files record binding and Finance-owned journal metadata advancement.
use crate::{
    field_attachment::{FieldAttachmentBindResponse, FieldAttachmentBindResult},
    finance_field_journal_files::{
        FinanceFieldJournalIntakeAuthority, FinanceFieldJournalIntakeSources,
        JournalEvidenceVersionAuthority,
    },
    finance_journal_actor::command_actor,
    finance_journal_create::{FinanceJournalCreateConfiguration, Ids},
};
use erpx_server_files::public::{field_visit_bind::*, field_visit_intake::*};
use erpx_server_finance::public::field_journal_evidence::*;
use erpx_server_identity_access::public::{
    effective_authority::{AuthorizedWorkContext, HoldingWorkSourcePreparation},
    selected_command_authority::require_selected_actor_current,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use erpx_server_workflow::public::{
    commands::*, field_intake_command_evidence::FieldIntakeCommandClock,
};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
type Guard<'a, C> = CurrentFieldIntake<'a, FinanceFieldJournalIntakeAuthority<'a, C>>;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ATTACHMENT_SET_CHANGED",
        "The original journal file set changed.",
    )
}
struct Authority<'a, C> {
    current: &'a AuthorizedWorkContext,
    config: &'a FinanceJournalCreateConfiguration,
    clock: &'a C,
    request: &'a WorkflowRequest,
}
impl<C: Clock + Send + Sync> PgCommandAuthority for Authority<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        FinanceFieldJournalIntakeSources.prepare(c).await
    }
    async fn authorize(
        &mut self,
        c: &mut PgConnection,
        r: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        if r.key != self.request.key
            || r.operation != "attachment.record.bind"
            || r.record_id != self.request.record_id
            || r.input != self.request.input
        {
            return Err(changed().into());
        }
        self.current.require_permission("finance.read")?;
        self.current.require_permission("module.finance.read")?;
        require_selected_actor_current(c, self.current, self.clock.now().get()).await?;
        if self.current.holding_id() != &self.config.holding
            || self.current.selection().workspace().company_id != self.config.company
        {
            return Err(changed().into());
        }
        Ok(command_actor(self.current)?)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        a: &CommandActor,
        r: &WorkflowRequest,
        disposition: CommandDisposition,
    ) -> Result<(), Error> {
        if self.authorize(c, r).await? != *a {
            return Err(changed().into());
        }
        let source =
            FinanceFieldJournalIntakeAuthority::for_read(self.current, self.config, self.clock);
        let guard = prepare_field_intake(
            c,
            &source,
            self.current,
            &r.record_id,
            disposition == CommandDisposition::Execute,
            self.clock.now().get(),
        )
        .await?;
        guard.require_current(c, self.clock.now().get()).await
    }
}
struct Effects<'a, C: Clock + Send + Sync> {
    current: &'a AuthorizedWorkContext,
    config: &'a FinanceJournalCreateConfiguration,
    clock: &'a C,
    fixed: &'a FieldIntakeCommandClock,
    lookup: &'a FinanceFieldJournalIntakeAuthority<'a, C>,
    manage: &'a FinanceFieldJournalIntakeAuthority<'a, C>,
    read: &'a Guard<'a, C>,
    target: &'a str,
    set_id: &'a str,
    expected: i64,
    write: Option<Guard<'a, C>>,
    set: Option<CurrentFieldBindSet>,
    written: Option<CurrentFieldBindSet>,
    metadata: Option<PreparedFieldJournalEvidenceVersion>,
    items: Vec<FieldBindManifestItem>,
}
impl<C: Clock + Send + Sync> PgOwnerCommandEffects for Effects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        FinanceFieldJournalIntakeSources.prepare(c).await
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let write = prepare_field_intake(
            c,
            self.manage,
            self.current,
            self.target,
            true,
            self.clock.now().get(),
        )
        .await?;
        self.read.require_current(c, self.clock.now().get()).await?;
        if self.read.scope().version != self.expected || write.scope().version != self.expected {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "ATTACHMENT_TARGET_CHANGED",
                "The journal changed before evidence was bound.",
            )
            .into());
        }
        let set = read_owner_record_bind_set(
            c,
            &self.config.company,
            self.set_id,
            IntakeOwner::FieldJournal,
        )
        .await?;
        if set.data().target_id != self.target {
            return Err(changed().into());
        }
        let items =
            prepare_owner_record_bind(set.data(), self.expected, IntakeOwner::FieldJournal)?;
        let mut policy = JournalEvidenceVersionAuthority {
            current: self.current,
            config: self.config,
            clock: self.clock,
        };
        let metadata = prepare_field_journal_evidence_version(
            c,
            &mut policy,
            self.target,
            self.expected,
            self.fixed.now().get(),
        )
        .await?;
        let version = metadata.version();
        let result = CommandJson::from_serializable(
            &serde_json::json!({"setId":self.set_id,"version":version,"manifest":items}),
        )?;
        self.write = Some(write);
        self.set = Some(set);
        self.items = items;
        self.metadata = Some(metadata);
        Ok(WorkflowOutput {
            version,
            result,
            event_scope: Some(CommandEventScope {
                company_id: self.config.company.clone(),
                branch_id: CommandField::Null,
                department_id: CommandField::Null,
            }),
        })
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        r: &WorkflowRequest,
    ) -> Result<(), Error> {
        self.read.require_current(c, self.clock.now().get()).await?;
        self.write
            .as_ref()
            .ok_or_else(changed)?
            .require_current(c, self.clock.now().get())
            .await?;
        self.set
            .as_ref()
            .ok_or_else(changed)?
            .apply(
                c,
                &self.items,
                self.current.user_id().as_str(),
                &r.key,
                self.fixed.now().get(),
            )
            .await?;
        let mut policy = JournalEvidenceVersionAuthority {
            current: self.current,
            config: self.config,
            clock: self.clock,
        };
        self.metadata
            .as_ref()
            .ok_or_else(changed)?
            .apply(c, &mut policy)
            .await?;
        self.written = Some(
            read_owner_record_bind_set(
                c,
                &self.config.company,
                self.set_id,
                IntakeOwner::FieldJournal,
            )
            .await?,
        );
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
        let metadata = self.metadata.as_ref().ok_or_else(changed)?;
        metadata.require_after(c).await?;
        self.lookup
            .require_after_version(
                c,
                self.current,
                self.target,
                false,
                metadata.version(),
                self.fixed.now().get(),
            )
            .await?;
        self.manage
            .require_after_version(
                c,
                self.current,
                self.target,
                true,
                metadata.version(),
                self.fixed.now().get(),
            )
            .await?;
        let set = self.written.as_ref().ok_or_else(changed)?;
        set.require_current(c).await?;
        if set.data().target_id != self.target {
            return Err(changed().into());
        }
        verify_field_bind_replay(c, set, &r.key, self.current.user_id().as_str(), &self.items).await
    }
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResultData {
    set_id: String,
    version: i64,
    manifest: Vec<FieldBindManifestItem>,
}
pub(crate) async fn bind<C: Clock + Send + Sync>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    config: &FinanceJournalCreateConfiguration,
    clock: &C,
    set_id: &str,
    key: &str,
    expected: i64,
) -> Result<FieldAttachmentBindResponse, Error> {
    validate_key(key)?;
    let set =
        read_owner_record_bind_set(c, &config.company, set_id, IntakeOwner::FieldJournal).await?;
    if set.data().expected_version != expected {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "ATTACHMENT_SET_STALE",
            "Create a record-document set for the current record version.",
        )
        .into());
    }
    let target = set.data().target_id.clone();
    let lookup = FinanceFieldJournalIntakeAuthority::for_read(current, config, clock);
    let manage = FinanceFieldJournalIntakeAuthority::for_manage(current, config, clock);
    let read = prepare_field_intake(c, &lookup, current, &target, false, clock.now().get()).await?;
    let identity = WorkflowRecordIdentity::from_utf16(target.encode_utf16().collect());
    let raw = format!(
        "{{\"setId\":{},\"expectedTargetVersion\":{expected}}}",
        serde_json::to_string(set_id).map_err(|_| changed())?
    );
    let json = CommandJson::parse(&raw)?;
    let request = WorkflowRequest {
        key: key.into(),
        operation: "attachment.record.bind".into(),
        record_id: identity.physical_text().into(),
        input: json.clone(),
    };
    let ordered = SourceOrderedWorkflowRequest {
        key: key.into(),
        operation: request.operation.clone(),
        record_id: identity,
        input: json,
    };
    let fixed = FieldIntakeCommandClock::sample(clock)?;
    let mut authority = Authority {
        current,
        config,
        clock,
        request: &request,
    };
    let mut effects = Effects {
        current,
        config,
        clock,
        fixed: &fixed,
        lookup: &lookup,
        manage: &manage,
        read: &read,
        target: &target,
        set_id,
        expected,
        write: None,
        set: None,
        written: None,
        metadata: None,
        items: Vec::new(),
    };
    let mut ids = Ids;
    let outcome = if let Some(result) =
        PgCommandLedger::new(config.company.clone(), c, &mut authority, &fixed, &mut ids)
            .replay_source_ordered(&ordered, &mut effects)
            .await?
    {
        result
    } else {
        PgCommandLedger::new(config.company.clone(), c, &mut authority, &fixed, &mut ids)
            .execute_source_ordered(&ordered, &mut effects)
            .await?
    };
    let result: ResultData =
        serde_json::from_str(outcome.result.as_str()).map_err(|_| changed())?;
    if result.set_id != set_id || expected.checked_add(1) != Some(result.version) {
        return Err(changed().into());
    }
    if outcome.replayed {
        let set = read_owner_record_bind_set(c, &config.company, set_id, IntakeOwner::FieldJournal)
            .await?;
        if set.data().target_id != target || read.scope().version < result.version {
            return Err(changed().into());
        }
        verify_field_bind_replay(c, &set, key, current.user_id().as_str(), &result.manifest)
            .await?;
        read.require_current(c, clock.now().get()).await?;
    } else {
        effects
            .retain(c, &command_actor(current)?, &request)
            .await?;
        authority
            .verify_command(
                c,
                &command_actor(current)?,
                &request,
                CommandDisposition::Execute,
            )
            .await?;
    }
    Ok(FieldAttachmentBindResponse {
        data: FieldAttachmentBindResult {
            set_id: result.set_id,
            version: result.version,
            manifest: crate::field_attachment_bind::manifest(&result.manifest),
        },
        context: crate::field_service_reservation_read::context(current)?,
    })
}
