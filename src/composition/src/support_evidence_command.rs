//! Support evidence command composition. Files owns the set/link/attachment
//! rows; Support owns the ticket version and evidence event. Both are mutated
//! in the same Workflow command transaction.
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness, current_package_catalogue,
};
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, WorkAuthorizationRequest,
        WorkPermissionRequirement,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, IdentifierSource,
    RecordId, TimestampMillis,
};
use erpx_server_support::public::support_evidence::{
    EvidenceActor, EvidenceBind, EvidenceBindPlan, EvidenceLinkFacts, EvidenceRevoke,
    EvidenceRevokePlan, EvidenceSetFacts, EvidenceTicket, plan_bind, plan_revoke,
};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandDisposition, CommandField, CommandJson, PgCommandAuthority,
    PgCommandLedger, PgOwnerCommandEffects, WorkAuthority, WorkflowOutput, WorkflowRequest,
};
use serde::Serialize;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

const ELIGIBLE: &str = "SELECT EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=$1 AND a.user_id=$2 AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'support.work') AND iam_role_allows(a.role,a.company_id,'support.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=$3) OR (a.scope_kind='department' AND a.scope_node_id=$4)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))";

const ATTACHMENT_WITHDRAWABLE: &str = "SELECT EXISTS(SELECT 1 FROM attachments WHERE id=$1 AND owner_type='support_ticket' AND owner_id=$2 AND revoked_at IS NULL AND legal_hold=0)";

#[derive(Clone, Debug)]
enum EvidenceWrite {
    Bind {
        ticket_id: String,
        input: EvidenceBind,
    },
    Revoke {
        ticket_id: String,
        input: EvidenceRevoke,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceResult {
    pub id: String,
    pub version: i64,
}

#[derive(Clone)]
pub struct SupportEvidenceCommand<C> {
    pub authorizer: Arc<HoldingWorkAuthorizer>,
    pub holding: HoldingId,
    pub company: CompanyId,
    pub catalogue: Arc<CommercialCatalogue>,
    pub clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> SupportEvidenceCommand<C> {
    #[allow(clippy::too_many_arguments)]
    pub async fn bind<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        ticket_id: String,
        input: EvidenceBind,
        wire: CommandJson,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        self.run(
            host,
            proof,
            workspace,
            EvidenceWrite::Bind { ticket_id, input },
            wire,
            key,
        )
        .await
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn revoke<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        ticket_id: String,
        input: EvidenceRevoke,
        wire: CommandJson,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        self.run(
            host,
            proof,
            workspace,
            EvidenceWrite::Revoke { ticket_id, input },
            wire,
            key,
        )
        .await
    }
    async fn run<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        write: EvidenceWrite,
        wire: CommandJson,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let expected_company = self.company.clone();
        let company = self.company.clone();
        let clock = self.clock.clone();
        let expected = key.clone();
        let (operation, record_id) = match &write {
            EvidenceWrite::Bind { ticket_id, .. } => {
                ("support.evidence.bind".to_owned(), ticket_id.clone())
            }
            EvidenceWrite::Revoke { ticket_id, .. } => {
                ("support.evidence.revoke".to_owned(), ticket_id.clone())
            }
        };
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                workspace()?,
                WorkAuthorizationRequest {
                    permission: WorkPermissionRequirement::AnyOf(vec![
                        "support.manage".into(),
                        "support.work".into(),
                    ]),
                    module: Some("support".into()),
                    destination: operation.clone(),
                    api: true,
                },
                SupportEvidenceSources,
                move |c, current| {
                    let expected_company = expected_company.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let expected = expected.clone();
                    let operation = operation.clone();
                    let record_id = record_id.clone();
                    let write = write.clone();
                    let wire = wire.clone();
                    Box::pin(async move {
                        if current.selection().workspace().company_id != expected_company {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "SUPPORT_EVIDENCE_SCOPE_REQUIRED",
                                "Use the current Company.",
                            )
                            .into());
                        }
                        let clock_ref = clock.as_ref();
                        let now = clock_ref.now();
                        let actor = evidence_actor(current, now)?;
                        let request = WorkflowRequest {
                            key: expected.as_str().to_owned(),
                            operation,
                            record_id,
                            input: wire.clone(),
                        };
                        let mut authority = Authority {
                            current,
                            request: &request,
                            clock: clock_ref,
                            purchases: None,
                        };
                        let mut effects = Effects {
                            company: &company,
                            clock: clock_ref,
                            request: &request,
                            write: &write,
                            actor,
                            prepared: None,
                        };
                        let mut ids = Ids;
                        let outcome = PgCommandLedger::new(
                            company.clone(),
                            c,
                            &mut authority,
                            clock_ref,
                            &mut ids,
                        )
                        .execute(&request, &mut effects)
                        .await?;
                        Ok(outcome.result)
                    })
                },
            )
            .await
    }
}

fn evidence_actor(
    current: &AuthorizedWorkContext,
    now: TimestampMillis,
) -> Result<EvidenceActor, BusinessError> {
    if now.get() >= current.expires_at() {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SUPPORT_EVIDENCE_AUTHORITY_CHANGED",
            "Your current Support authority is unavailable.",
        ));
    }
    let workspace = current.selection().workspace();
    Ok(EvidenceActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: workspace.company_id.as_str().to_owned(),
        branch_id: workspace.branch_id.as_ref().map(|v| v.as_str().to_owned()),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|v| v.as_str().to_owned()),
        can_read: current.has_permission("support.read") || current.has_permission("records.read"),
        can_work: current.has_permission("support.work"),
        can_manage: current.has_permission("support.manage"),
        same_person_ids: Vec::new(),
    })
}

#[derive(Clone)]
struct SupportEvidenceSources;
impl erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation
    for SupportEvidenceSources
{
    fn prepare<'c>(
        &'c self,
        c: &'c mut PgConnection,
    ) -> erpx_server_runtime::public::postgres::PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            erpx_server_commercial::public::purchases::lock_commercial_authority_sources(c).await?;
            sqlx::query("LOCK TABLE support_tickets,support_ticket_events,attachment_sets,attachment_links,attachment_events,attachments,staff_notifications IN SHARE MODE")
                .execute(&mut *c)
                .await?;
            Ok(())
        })
    }
}

struct Authority<'a, C> {
    current: &'a AuthorizedWorkContext,
    request: &'a WorkflowRequest,
    clock: &'a C,
    purchases: Option<Purchases>,
}
impl<C: Clock + Sync> PgCommandAuthority for Authority<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation::prepare(
            &SupportEvidenceSources,
            c,
        )
        .await
    }
    async fn authorize(
        &mut self,
        _: &mut PgConnection,
        r: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        if r.key != self.request.key
            || r.operation != self.request.operation
            || r.record_id != self.request.record_id
            || r.input.as_str() != self.request.input.as_str()
        {
            return Err(changed().into());
        }
        Ok(actor(self.current, self.clock.now())?)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        a: &CommandActor,
        _: &WorkflowRequest,
        disposition: CommandDisposition,
    ) -> Result<(), Error> {
        if &actor(self.current, self.clock.now())? != a {
            return Err(changed().into());
        }
        match &self.purchases {
            Some(p) => p.check(c, self.current, self.clock.now()).await?,
            None => {
                self.purchases =
                    Some(Purchases::read(c, self.current, disposition, self.clock.now()).await?)
            }
        }
        Ok(())
    }
}

struct Purchases {
    catalogue: CommercialCatalogue,
    history: PurchaseWitness,
    new_work: Option<PurchaseWitness>,
}
impl Purchases {
    async fn read(
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        disposition: CommandDisposition,
        now: TimestampMillis,
    ) -> Result<Self, Error> {
        let catalogue = current_package_catalogue()?;
        let mut reader = PgCompanyPurchaseReader::new(
            current.holding_id().clone(),
            current.selection().workspace().company_id.clone(),
            c,
            &catalogue,
        );
        let inspect = |purpose| PurchaseRequest {
            capabilities: vec!["support.evidence".into()],
            purpose,
            mode: PackageEnforcementMode::Enforced,
        };
        let history = reader
            .inspect(&inspect(CapabilityPurpose::History), now)
            .await?
            .require_allowed()?
            .clone();
        let new_work = if disposition == CommandDisposition::Execute {
            Some(
                reader
                    .inspect(&inspect(CapabilityPurpose::NewWork), now)
                    .await?
                    .require_allowed()?
                    .clone(),
            )
        } else {
            None
        };
        Ok(Self {
            catalogue,
            history,
            new_work,
        })
    }
    async fn check(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        let mut reader = PgCompanyPurchaseReader::new(
            current.holding_id().clone(),
            current.selection().workspace().company_id.clone(),
            c,
            &self.catalogue,
        );
        reader
            .revalidate(&self.history, now)
            .await?
            .require_allowed()?;
        if let Some(witness) = &self.new_work {
            reader.revalidate(witness, now).await?.require_allowed()?;
        }
        Ok(())
    }
}

struct Effects<'a, C> {
    company: &'a CompanyId,
    clock: &'a C,
    request: &'a WorkflowRequest,
    write: &'a EvidenceWrite,
    actor: EvidenceActor,
    prepared: Option<Prepared>,
}

enum Prepared {
    Bind(EvidenceBindPlan),
    Revoke(EvidenceRevokePlan),
}

type TicketRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    Option<String>,
    i64,
);
type SetRow = (
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    String,
    i64,
    String,
    i64,
);

impl<C: Clock + Sync> PgOwnerCommandEffects for Effects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation::prepare(
            &SupportEvidenceSources,
            c,
        )
        .await
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let (ticket_id, input_reason) = match self.write {
            EvidenceWrite::Bind { ticket_id, input } => (ticket_id, input.reason.as_str()),
            EvidenceWrite::Revoke { ticket_id, input } => (ticket_id, input.reason.as_str()),
        };
        let ticket = load_ticket(c, self.company, ticket_id).await?;
        let eligible: bool = sqlx::query_scalar(ELIGIBLE)
            .bind(self.company.as_str())
            .bind(&self.actor.user_id)
            .bind(ticket.branch_id.as_deref())
            .bind(ticket.department_id.as_deref())
            .fetch_one(&mut *c)
            .await?;
        if !eligible {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "SUPPORT_EVIDENCE_ASSIGNMENT",
                "Your current employment and assignment no longer permit changes to this case evidence.",
            )
            .into());
        }
        let _ = input_reason;
        match self.write {
            EvidenceWrite::Bind { input, .. } => {
                let set_id = input
                    .attachment_set_ids
                    .first()
                    .cloned()
                    .unwrap_or_default();
                let set = load_set(c, self.company, &set_id).await?;
                let plan = plan_bind(
                    &self.actor,
                    &ticket,
                    &set,
                    input,
                    self.request.key.as_str(),
                    self.clock.now().get(),
                )?;
                let result = EvidenceResult {
                    id: plan.ticket_id.clone(),
                    version: plan.ticket_version,
                };
                self.prepared = Some(Prepared::Bind(plan));
                Ok(WorkflowOutput {
                    version: result.version,
                    result: CommandJson::from_serializable(&result)?,
                    event_scope: None,
                })
            }
            EvidenceWrite::Revoke { input, .. } => {
                let withdrawable: bool = sqlx::query_scalar(ATTACHMENT_WITHDRAWABLE)
                    .bind(&input.attachment_id)
                    .bind(&ticket.id)
                    .fetch_one(&mut *c)
                    .await?;
                if !withdrawable {
                    return Err(BusinessError::new(
                        FailureKind::Conflict,
                        "CONFLICT",
                        "The evidence is held or no longer withdrawable.",
                    )
                    .into());
                }
                let plan = plan_revoke(&self.actor, &ticket, input, self.clock.now().get())?;
                let result = EvidenceResult {
                    id: plan.ticket_id.clone(),
                    version: plan.ticket_version,
                };
                self.prepared = Some(Prepared::Revoke(plan));
                Ok(WorkflowOutput {
                    version: result.version,
                    result: CommandJson::from_serializable(&result)?,
                    event_scope: None,
                })
            }
        }
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        match self.prepared.take().ok_or_else(changed)? {
            Prepared::Bind(plan) => {
                apply_bind(c, self.request.key.as_str(), &plan).await?;
            }
            Prepared::Revoke(plan) => {
                apply_revoke(c, self.request.key.as_str(), &plan).await?;
            }
        }
        Ok(())
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let Some(prepared) = &self.prepared else {
            return Ok(());
        };
        match prepared {
            Prepared::Bind(plan) => {
                let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attachment_sets s JOIN support_tickets t ON t.id=$2 JOIN support_ticket_events e ON e.ticket_id=t.id AND e.version=t.version AND e.action='attachment-bind' WHERE s.id=$1 AND s.state='bound' AND s.bound_command_id=$3 AND t.version=$4)")
                    .bind(&plan.set_id)
                    .bind(&plan.ticket_id)
                    .bind(self.request.key.as_str())
                    .bind(plan.ticket_version)
                    .fetch_one(&mut *c)
                    .await?;
                if !exists {
                    return Err(changed().into());
                }
            }
            Prepared::Revoke(plan) => {
                let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attachments a JOIN support_ticket_events e ON e.ticket_id=$2 AND e.version=$3 AND e.action='attachment-revoke' WHERE a.id=$1 AND a.revoked_at IS NOT NULL)")
                    .bind(&plan.attachment_id)
                    .bind(&plan.ticket_id)
                    .bind(plan.ticket_version)
                    .fetch_one(&mut *c)
                    .await?;
                if !exists {
                    return Err(changed().into());
                }
            }
        }
        Ok(())
    }
}

async fn load_ticket(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<EvidenceTicket, Error> {
    let row: Option<TicketRow> = sqlx::query_as(
        "SELECT id,company_id,branch_id,department_id,status,source_kind,owner_id,version FROM support_tickets WHERE id=$1 AND company_id=$2",
    )
    .bind(id)
    .bind(company.as_str())
    .fetch_optional(&mut *c)
    .await?;
    let Some((id, company_id, branch_id, department_id, status, source_kind, owner_id, version)) =
        row
    else {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "SUPPORT_TICKET_NOT_FOUND",
            "Ticket not found in this scope.",
        )
        .into());
    };
    Ok(EvidenceTicket {
        id,
        company_id,
        branch_id,
        department_id,
        status,
        source_kind,
        owner_id,
        version,
    })
}

async fn load_set(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<EvidenceSetFacts, Error> {
    let row: Option<SetRow> = sqlx::query_as(
        "SELECT id,company_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,state,required_count FROM attachment_sets WHERE id=$1 AND company_id=$2",
    )
    .bind(id)
    .bind(company.as_str())
    .fetch_optional(&mut *c)
    .await?;
    let Some((
        id,
        company_id,
        target_type,
        target_id,
        component_kind,
        component_ref,
        purpose_key,
        expected_target_version,
        state,
        required_count,
    )) = row
    else {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "ATTACHMENT_SET_NOT_FOUND",
            "File set not found.",
        )
        .into());
    };
    let links: Vec<(String, String, String, String, String, i64)> = sqlx::query_as(
        "SELECT l.id,l.attachment_id,a.source_state,a.checksum_sha256,a.media_type,a.bytes FROM attachment_links l JOIN attachments a ON a.id=l.attachment_id WHERE l.set_id=$1 AND l.state='draft' AND a.revoked_at IS NULL ORDER BY l.created_at,l.id",
    )
    .bind(&id)
    .fetch_all(&mut *c)
    .await?;
    Ok(EvidenceSetFacts {
        id,
        company_id,
        target_type,
        target_id,
        component_kind,
        component_ref,
        purpose_key,
        expected_target_version,
        state,
        required_count,
        links: links
            .into_iter()
            .map(
                |(id, attachment_id, source_state, checksum_sha256, media_type, bytes)| {
                    EvidenceLinkFacts {
                        id,
                        attachment_id,
                        source_state,
                        checksum_sha256,
                        media_type,
                        bytes,
                    }
                },
            )
            .collect(),
    })
}

async fn apply_bind(c: &mut PgConnection, key: &str, plan: &EvidenceBindPlan) -> Result<(), Error> {
    let updated = sqlx::query("UPDATE support_tickets SET version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3 AND version=$4")
        .bind(plan.event_at)
        .bind(&plan.ticket_id)
        .bind(&plan.company_id)
        .bind(plan.ticket_version - 1)
        .execute(&mut *c)
        .await?
        .rows_affected();
    if updated != 1 {
        return Err(changed().into());
    }
    for link in &plan.links {
        let updated = sqlx::query("UPDATE attachment_links SET state='bound',bound_at=$1,binding_type='support_action',binding_id=$2 WHERE id=$3 AND set_id=$4 AND state='draft'")
            .bind(plan.event_at)
            .bind(&plan.binding_id)
            .bind(link)
            .bind(&plan.set_id)
            .execute(&mut *c)
            .await?
            .rows_affected();
        if updated != 1 {
            return Err(changed().into());
        }
    }
    let updated = sqlx::query("UPDATE attachment_sets SET state='bound',bound_at=$1,bound_command_id=$2,updated_at=$1 WHERE id=$3 AND company_id=$4 AND state='draft'")
        .bind(plan.event_at)
        .bind(key)
        .bind(&plan.set_id)
        .bind(&plan.company_id)
        .execute(&mut *c)
        .await?
        .rows_affected();
    if updated != 1 {
        return Err(changed().into());
    }
    sqlx::query("INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,'set.bound',$5,$6)")
        .bind(format!("attachment-bind-{key}"))
        .bind(&plan.company_id)
        .bind(&plan.set_id)
        .bind(&plan.actor_id)
        .bind(
            serde_json::to_string(&serde_json::json!({
                "bindingType": "support_action",
                "bindingId": plan.binding_id,
                "manifest": serde_json::from_str::<serde_json::Value>(&plan.manifest_json).map_err(|_| changed())?,
            }))
            .map_err(|_| changed())?,
        )
        .bind(plan.event_at)
        .execute(&mut *c)
        .await?;
    insert_ticket_event(
        c,
        key,
        "attachment-bind",
        &plan.ticket_id,
        &plan.actor_id,
        &plan.reason,
        &serde_json::json!({
            "manifest": serde_json::from_str::<serde_json::Value>(&plan.manifest_json).map_err(|_| changed())?,
        }),
    )
    .await?;
    Ok(())
}

async fn apply_revoke(
    c: &mut PgConnection,
    key: &str,
    plan: &EvidenceRevokePlan,
) -> Result<(), Error> {
    let updated = sqlx::query("UPDATE support_tickets SET version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3 AND version=$4")
        .bind(plan.event_at)
        .bind(&plan.ticket_id)
        .bind(&plan.company_id)
        .bind(plan.ticket_version - 1)
        .execute(&mut *c)
        .await?
        .rows_affected();
    if updated != 1 {
        return Err(changed().into());
    }
    let updated = sqlx::query("UPDATE attachments SET revoked_at=$1 WHERE id=$2 AND owner_type='support_ticket' AND owner_id=$3 AND revoked_at IS NULL AND legal_hold=0")
        .bind(plan.event_at)
        .bind(&plan.attachment_id)
        .bind(&plan.ticket_id)
        .execute(&mut *c)
        .await?
        .rows_affected();
    if updated != 1 {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "CONFLICT",
            "The evidence is held or no longer withdrawable.",
        )
        .into());
    }
    insert_ticket_event(
        c,
        key,
        "attachment-revoke",
        &plan.ticket_id,
        &plan.actor_id,
        &plan.reason,
        &serde_json::json!({
            "attachmentId": plan.attachment_id,
            "expectedVersion": plan.ticket_version - 1,
        }),
    )
    .await?;
    Ok(())
}

async fn insert_ticket_event(
    c: &mut PgConnection,
    key: &str,
    action: &str,
    ticket_id: &str,
    actor_id: &str,
    reason: &str,
    evidence: &serde_json::Value,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO support_ticket_events(id,ticket_id,version,action,actor_id,reason,owner_id,due_date,status,created_at,evidence_json) SELECT $1,id,version,$2,$3,$4,owner_id,due_date,status,updated_at,$5::jsonb FROM support_tickets WHERE id=$6")
        .bind(format!("support-evidence-{key}"))
        .bind(action)
        .bind(actor_id)
        .bind(reason)
        .bind(serde_json::to_string(evidence).map_err(|_| changed())?)
        .bind(ticket_id)
        .execute(&mut *c)
        .await?;
    Ok(())
}

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SUPPORT_EVIDENCE_CHANGED",
        "The Support evidence changed. Reload before retrying.",
    )
}

fn actor(
    current: &AuthorizedWorkContext,
    now: TimestampMillis,
) -> Result<CommandActor, BusinessError> {
    current.require_permission("records.read")?;
    if now.get() >= current.expires_at() {
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
        iam: CommandField::Value(CommandJson::from_serializable(
            &erpx_server_identity_access::public::command_metadata::project_command_iam(current)?,
        )?),
        assignment_scope_kind: match a.scope_kind {
            erpx_server_identity_access::public::work_context::WorkScopeKind::Holding => "holding",
            erpx_server_identity_access::public::work_context::WorkScopeKind::Branch => "branch",
            erpx_server_identity_access::public::work_context::WorkScopeKind::Department => {
                "department"
            }
        }
        .into(),
        assignment_scope_node_id: a.scope_node_id.as_ref().map(|v| v.as_str().into()),
    })
}

struct Ids;
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("generated UUID")
    }
}
