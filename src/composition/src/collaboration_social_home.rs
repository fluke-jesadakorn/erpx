use super::{
    AuthorizedWorkContext, BusinessError, Clock, CompanyId, Error, FailureKind,
    HoldingWorkspaceRequest, PackageEnforcementMode, PgConnection, PgSocialUnit, SocialRecordsPort,
    SocialScope, SocialService, TimestampMillis, WorkAuthorizationRequest, WorkScopeKind, business,
    current_package_catalogue, display_name, scope_error, social_scope,
};
use erpx_server_collaboration::public::inbox::encode_uri_component;
use erpx_server_collaboration::public::social::{
    SocialHomeAdditions, SocialHomeCandidate, SocialHomePolicy, SocialHomePolicyFuture,
    SocialHomeSelection, social_audience_teams,
};
use erpx_server_collaboration::public::social_http::{
    SocialAudienceOptionsData, SocialPageData, SocialPresentationData,
};
use erpx_server_commercial::public::capability_decision::{
    PgCapabilityDecisionReader, PurchasedAccess, purchased_accesses,
};
use erpx_server_commercial::public::purchases::CapabilityPurpose;
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::WorkPermissionRequirement,
    pms_social_event_recipients::{pms_social_conversation_readable, pms_social_source_readable},
};
use erpx_server_operations::public::record_source::{SocialTaskSource, read_social_task_source};
use erpx_server_organization::public::organization_nodes::{
    social_audience_branches, social_department_name,
};
use erpx_server_pms::public::pms_social_event::{PmsSocialSource, pms_social_source_for_company};
use erpx_server_projects::public::staff_facts::{
    SocialProjectAccess, SocialProjectReference, social_project_audiences,
    social_project_reference, staff_project,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone)]
pub(super) struct PageInput {
    pub(super) post_id: Option<String>,
    pub(super) tag: Option<String>,
    pub(super) skip_milestones: bool,
    pub(super) source_only: bool,
    pub(super) context: Option<String>,
}

impl<C: Clock + Send + Sync + 'static> SocialService<C> {
    pub(super) async fn page(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        input: PageInput,
    ) -> Result<SocialPageData, Error> {
        if input.source_only && input.post_id.is_none() {
            return Err(
                BusinessError::invalid("SOCIAL_THREAD_REQUIRED", "Choose a conversation.").into(),
            );
        }
        let direct_thread = input.post_id.is_some();
        let permission = if direct_thread {
            WorkPermissionRequirement::AnyOf(vec![
                "records.read".into(),
                "pms.read".into(),
                "finance.read".into(),
            ])
        } else {
            "records.read".into()
        };
        let module = (!direct_thread).then(|| "collaboration".into());
        let holding = self.configuration.holding.clone();
        let company = selection.company_id.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || {
                    let mut request = selection;
                    request.mutation = false;
                    Ok(request)
                },
                WorkAuthorizationRequest {
                    permission,
                    module,
                    destination: "feed".into(),
                    api: true,
                },
                super::CollaborationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let input = input.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_error().into());
                        }
                        present_page(connection, current, company, clock.now().get(), input).await
                    })
                },
            )
            .await
    }
}

async fn present_page(
    connection: &mut PgConnection,
    current: &AuthorizedWorkContext,
    company: CompanyId,
    now: i64,
    input: PageInput,
) -> Result<SocialPageData, Error> {
    let context = input.context.unwrap_or_else(|| {
        let selected = current.selection();
        let workspace = selected.workspace();
        digest(&json!([
            company.as_str(),
            workspace.branch_id.as_ref().map(|id| id.as_str()),
            workspace.department_id.as_ref().map(|id| id.as_str()),
            current.user_id().as_str(),
            selected.assignment().id.as_str(),
            selected.assignment().version,
            selected.revision(),
        ]))
    });
    let purchase =
        PagePurchase::load(connection, &company, now, &context, input.post_id.is_some()).await?;
    let name = display_name(connection, &company, current.user_id().as_str()).await;
    let ai_name =
        erpx_server_agent::public::providers::social_ai_display_name(connection, &company).await?;
    let scope = social_scope(current, &company, name, now, false)?;
    let mut records = SocialRecordsPort {
        actor: crate::records::records_actor(current)?,
        now,
    };
    let mut policy = PagePolicy {
        current,
        purchase,
        ai_name,
        reading_origin: false,
        direct_thread: input.post_id.is_some(),
        require_source_only: input.source_only,
        source_only: false,
    };
    let mut unit = PgSocialUnit::read_only(connection, scope.clone(), &mut records);
    let posts = match input.post_id.as_deref() {
        Some(post_id) => {
            unit.home_posts(SocialHomeSelection::Thread { post_id }, &mut policy)
                .await?
        }
        None => {
            unit.home_posts(
                SocialHomeSelection::Conversations {
                    tag: input.tag.as_deref(),
                },
                &mut policy,
            )
            .await?
        }
    };
    let milestones = if input.post_id.is_some() || input.skip_milestones {
        Vec::new()
    } else {
        unit.home_posts(
            SocialHomeSelection::Milestones {
                tag: input.tag.as_deref(),
            },
            &mut policy,
        )
        .await?
    };
    let post = input.post_id.and_then(|_| posts.first().cloned());
    let options = audience_options(
        connection,
        current,
        &scope,
        &policy.purchase,
        policy.source_only,
    )
    .await?;
    Ok(SocialPageData {
        posts,
        milestones,
        options,
        source_only: policy.source_only,
        moderate: !policy.source_only
            && current.has_permission("manage_erpx.access")
            && current.selection().assignment().scope_kind == WorkScopeKind::Holding,
        presentation: Some(SocialPresentationData {
            context,
            purchase: policy.purchase.composer(),
        }),
        // The Feed page requests skipDay. Records owns the separate Day surface.
        initial_day: None,
        post,
    })
}

async fn audience_options(
    connection: &mut PgConnection,
    current: &AuthorizedWorkContext,
    scope: &SocialScope,
    purchase: &PagePurchase,
    source_only: bool,
) -> Result<SocialAudienceOptionsData, Error> {
    let mut data = SocialAudienceOptionsData {
        branches: Vec::new(),
        audiences: Vec::new(),
        projects: Vec::new(),
    };
    if source_only {
        return Ok(data);
    }
    let kind = current.selection().assignment().scope_kind;
    let branches =
        social_audience_branches(connection, &scope.company_id, scope.branch_id.as_deref()).await?;
    let within = match kind {
        WorkScopeKind::Holding => {
            data.audiences
                .push(option("company", "company", "Everyone in the company"));
            ""
        }
        WorkScopeKind::Branch => " · in your branch",
        WorkScopeKind::Department => " · in your department",
    };
    if kind == WorkScopeKind::Department {
        if let Some(id) = scope.department_id.as_deref() {
            let label = social_department_name(connection, &scope.company_id, id)
                .await?
                .unwrap_or_else(|| "Your department".to_owned());
            data.audiences.push(option(
                format!("department:{id}"),
                "department",
                format!("Department · {label}"),
            ));
        }
    } else {
        data.audiences.extend(
            branches
                .iter()
                .filter(|(id, _)| {
                    kind == WorkScopeKind::Holding
                        || scope.branch_id.as_deref() == Some(id.as_str())
                })
                .map(|(id, name)| {
                    option(format!("branch:{id}"), "branch", format!("Branch · {name}"))
                }),
        );
    }
    data.branches = branches
        .into_iter()
        .map(|(id, display_name)| json!({"id": id, "display_name": display_name}))
        .collect();
    data.audiences.push(option(
        "participants",
        "participants",
        format!("Selected people{within}"),
    ));
    for (id, name) in social_audience_teams(connection, scope).await? {
        data.audiences.push(option(
            format!("team:{id}"),
            "team",
            format!("Team · {name}{within}"),
        ));
    }
    if !purchase.projects || purchase.project_granted(current, CapabilityPurpose::History) {
        for project in social_project_audiences(
            connection,
            scope.company_id.as_str(),
            scope.user_id.as_str(),
        )
        .await?
        {
            data.audiences.push(option(
                format!("project:{}", project.id),
                "project",
                format!("Project · {}{within}", project.title),
            ));
            data.projects
                .push(json!({"id": project.id, "title": project.title}));
        }
    }
    Ok(data)
}

fn option(value: impl AsRef<str>, kind: &str, label: impl AsRef<str>) -> Value {
    json!({"value": value.as_ref(), "kind": kind, "label": label.as_ref()})
}

fn digest(input: &Value) -> String {
    format!("{:x}", Sha256::digest(input.to_string().as_bytes()))
}

#[derive(Clone)]
struct PagePurchase {
    work: bool,
    projects: bool,
    pms: bool,
    release: bool,
    revision: String,
    access: BTreeMap<String, PurchasedAccess>,
}

fn enforcement(key: &str) -> Result<PackageEnforcementMode, BusinessError> {
    let mode = match std::env::var(key).as_deref() {
        Ok("shadow") => PackageEnforcementMode::Shadow,
        Ok("enforced") => PackageEnforcementMode::Enforced,
        Ok("disabled") | Err(std::env::VarError::NotPresent) => PackageEnforcementMode::Disabled,
        _ => {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "PACKAGE_ENFORCEMENT_CONFIGURATION",
                "Package enforcement is unavailable.",
            ));
        }
    };
    if mode != PackageEnforcementMode::Disabled
        && std::env::var("ERPX_ENVIRONMENT").ok().as_deref() != Some("local")
    {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "PACKAGE_ENFORCEMENT_ENVIRONMENT",
            "Package enforcement is unavailable in this environment.",
        ));
    }
    Ok(mode)
}

impl PagePurchase {
    async fn load(
        connection: &mut PgConnection,
        company: &CompanyId,
        now: i64,
        context: &str,
        inspect_thread: bool,
    ) -> Result<Self, Error> {
        use PackageEnforcementMode::{Disabled, Enforced};
        let work = enforcement("ERPX_APPS_PACKAGE_ENFORCEMENT")?;
        let projects = enforcement("ERPX_PROJECTS_PACKAGE_ENFORCEMENT")?;
        let pms = enforcement("ERPX_PMS_PACKAGE_ENFORCEMENT")?;
        let mut page = Self {
            work: work == Enforced,
            projects: projects == Enforced,
            pms: pms == Enforced,
            // This native read lives on the private development and OASIS adapters.
            // If run with production policy, an unbound release fails closed.
            release: std::env::var("NODE_ENV").ok().as_deref() != Some("production"),
            revision: String::new(),
            access: BTreeMap::new(),
        };
        if inspect_thread || [work, projects, pms].iter().any(|mode| *mode != Disabled) {
            let catalogue = current_package_catalogue()?;
            let facts = PgCapabilityDecisionReader::new(company.clone(), connection, &catalogue)
                .facts()
                .await
                .map_err(business)?;
            let codes = [
                "work.social",
                "work.actions",
                "work.advanced-routines",
                "projects.wbs",
                "projects.tasks",
                "projects.handover",
                "pms.reservations",
                "pms.front-desk",
                "pms.rooms",
                "pms.housekeeping",
                "pms.guest-accounts",
                "pms.night-audit",
                "finance.hotel-posting",
                "finance.accounting",
            ]
            .map(str::to_owned);
            page.access = purchased_accesses(
                &catalogue,
                &facts,
                company.as_str(),
                &codes,
                Enforced,
                TimestampMillis::new(now),
            )?;
            page.revision = digest(&json!([page.access["work.social"].revision, context]));
        }
        Ok(page)
    }

    fn enforce_source_history(&self) -> Self {
        Self {
            work: true,
            projects: true,
            pms: true,
            ..self.clone()
        }
    }

    fn allows(&self, capability: &str, purpose: CapabilityPurpose) -> bool {
        self.access
            .get(capability)
            .is_some_and(|access| match purpose {
                CapabilityPurpose::NewWork => access.new_work,
                CapabilityPurpose::SettleExisting => access.settle_existing,
                CapabilityPurpose::History => access.history,
            })
    }

    fn project_granted(&self, current: &AuthorizedWorkContext, purpose: CapabilityPurpose) -> bool {
        self.release
            && current.has_permission("records.read")
            && self.allows("projects.wbs", purpose)
    }

    fn pms_granted(
        &self,
        current: &AuthorizedWorkContext,
        source: &PmsSocialSource,
        purpose: CapabilityPurpose,
    ) -> bool {
        self.release
            && (source
                .capability()
                .is_some_and(|code| self.allows(code, purpose))
                || purpose == CapabilityPurpose::History
                    && matches!(source.kind.as_str(), "guest_account" | "transfer")
                    && current.has_permission("finance.read")
                    && (self.allows("finance.hotel-posting", purpose)
                        || self.allows("finance.accounting", purpose)))
    }

    fn composer(&self) -> Option<Value> {
        use CapabilityPurpose::{History, NewWork, SettleExisting};
        self.work.then(|| {
            json!({
                "newWork": self.release && self.allows("work.social", NewWork),
                "settleExisting": self.release && self.allows("work.social", SettleExisting),
                "history": self.release && self.allows("work.social", History),
                "revision": self.revision,
            })
        })
    }
}

enum LinkedSource {
    Project {
        id: String,
        exists: bool,
        handover_reader: bool,
    },
    Task(Option<SocialTaskSource>),
    Pms(Box<PmsSocialSource>),
}

impl LinkedSource {
    fn included(&self) -> bool {
        match self {
            Self::Project { exists, .. } => *exists,
            Self::Pms(..) => true,
            Self::Task(task) => task.as_ref().is_some_and(|task| task.project_id.is_some()),
        }
    }

    fn exists(&self) -> bool {
        match self {
            Self::Task(task) => task.is_some(),
            _ => self.included(),
        }
    }
}

struct PagePolicy<'a> {
    current: &'a AuthorizedWorkContext,
    purchase: PagePurchase,
    ai_name: String,
    reading_origin: bool,
    direct_thread: bool,
    require_source_only: bool,
    source_only: bool,
}

impl PagePolicy<'_> {
    async fn sources(
        &self,
        connection: &mut PgConnection,
        scope: &SocialScope,
        candidate: &SocialHomeCandidate,
    ) -> Result<Option<Vec<LinkedSource>>, Error> {
        let company = scope.company_id.as_str();
        let user = scope.user_id.as_str();
        let pms_ids = candidate
            .sources
            .iter()
            .filter(|source| source.kind == "pms_record")
            .map(|source| source.id.clone())
            .collect::<Vec<_>>();
        if !pms_social_conversation_readable(
            connection,
            company,
            user,
            &pms_ids,
            candidate.branch_id.as_deref(),
            candidate.department_id.as_deref(),
        )
        .await?
        {
            return Ok(None);
        }
        let mut result = Vec::with_capacity(candidate.sources.len());
        for source in &candidate.sources {
            result.push(match source.kind.as_str() {
                "staff_project" => LinkedSource::Project {
                    id: source.id.clone(),
                    exists: staff_project(connection, company, &source.id)
                        .await?
                        .is_some(),
                    handover_reader: social_project_reference(
                        connection,
                        company,
                        user,
                        &source.id,
                        SocialProjectAccess::Handover,
                    )
                    .await?
                    .is_some(),
                },
                "staff_task" => LinkedSource::Task(
                    read_social_task_source(
                        connection,
                        &scope.company_id,
                        &source.id,
                        user,
                        scope.now,
                    )
                    .await?,
                ),
                "pms_record" => {
                    let Some(hotel) =
                        pms_social_source_for_company(connection, &source.id, company).await?
                    else {
                        return Ok(None);
                    };
                    LinkedSource::Pms(Box::new(hotel))
                }
                _ => return Ok(None),
            });
        }
        if candidate.purpose == "handover"
            && result.iter().any(|source| match source {
                LinkedSource::Project {
                    handover_reader, ..
                } => !handover_reader,
                LinkedSource::Task(task) => {
                    task.as_ref().is_none_or(|task| !task.handover_participant)
                }
                LinkedSource::Pms(_) => false,
            })
        {
            return Ok(None);
        }
        Ok(Some(result))
    }

    fn admitted(
        &self,
        purchase: &PagePurchase,
        candidate: &SocialHomeCandidate,
        project_audience: bool,
        sources: &[LinkedSource],
        purpose: CapabilityPurpose,
        card: bool,
    ) -> bool {
        if purchase.projects {
            if candidate.audience_kind.as_deref() == Some("project")
                && (!project_audience || !purchase.project_granted(self.current, purpose))
            {
                return false;
            }
            if sources.iter().any(|source| {
                let reader = match source {
                    LinkedSource::Project {
                        exists: true,
                        handover_reader,
                        ..
                    } => *handover_reader,
                    LinkedSource::Task(Some(task)) if task.project_id.is_some() => {
                        task.handover_participant && purchase.allows("projects.tasks", purpose)
                    }
                    _ => return false,
                };
                !reader
                    || !purchase.project_granted(self.current, purpose)
                    || card
                        && purchase.work
                        && candidate.operational_handover
                        && !purchase.allows("projects.handover", purpose)
            }) {
                return false;
            }
        }
        if purchase.pms && sources.iter().any(|source| {
            matches!(source, LinkedSource::Pms(hotel) if !purchase.pms_granted(self.current, hotel, purpose))
        }) {
            return false;
        }
        if !purchase.work {
            return true;
        }
        if !purchase.release {
            return false;
        }
        if candidate.operational_handover {
            return sources.iter().all(|source| {
                let LinkedSource::Task(Some(task)) = source else {
                    return true;
                };
                task.project_id.is_some()
                    || task.preview_participant
                        && purchase.allows("work.actions", purpose)
                        && (task.template_id.is_none()
                            || purchase.allows("work.advanced-routines", purpose))
            });
        }
        let native = candidate.kind != "system"
            && (sources.is_empty() || sources.iter().any(|source| !source.included()));
        purchase.allows("work.social", purpose)
            || !native && sources.iter().all(LinkedSource::exists)
    }

    async fn task_visible(
        &mut self,
        connection: &mut PgConnection,
        scope: &SocialScope,
        current_post: &str,
        task: &SocialTaskSource,
    ) -> Result<bool, Error> {
        let purchase = &self.purchase;
        let history = CapabilityPurpose::History;
        if !task.preview_participant
            || task.project_id.is_some()
                && purchase.projects
                && (!purchase.project_granted(self.current, history)
                    || !purchase.allows("projects.tasks", history))
            || task.project_id.is_none()
                && purchase.work
                && (!purchase.release
                    || !purchase.allows("work.actions", history)
                    || task.template_id.is_some()
                        && !purchase.allows("work.advanced-routines", history))
        {
            return Ok(false);
        }
        let Some(origin) = task.origin_post_id.as_deref() else {
            return Ok(true);
        };
        if origin == current_post {
            return Ok(true);
        }
        let mut records = SocialRecordsPort {
            actor: crate::records::records_actor(self.current)?,
            now: scope.now,
        };
        let mut unit = PgSocialUnit::read_only(connection, scope.clone(), &mut records);
        self.reading_origin = true;
        let result = unit
            .home_posts(SocialHomeSelection::Thread { post_id: origin }, self)
            .await;
        self.reading_origin = false;
        match result {
            Ok(_) => Ok(true),
            Err(Error::Business(error)) if error.kind == FailureKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    async fn preview(
        &mut self,
        connection: &mut PgConnection,
        scope: &SocialScope,
        candidate: &SocialHomeCandidate,
        source: &LinkedSource,
    ) -> Result<Value, Error> {
        let history = CapabilityPurpose::History;
        let (kind, visible) = match source {
            LinkedSource::Project { id, .. } => {
                let project = if !self.purchase.projects
                    || self.purchase.project_granted(self.current, history)
                {
                    social_project_reference(
                        connection,
                        scope.company_id.as_str(),
                        scope.user_id.as_str(),
                        id,
                        SocialProjectAccess::AudienceOrPreview,
                    )
                    .await?
                } else {
                    None
                };
                (
                    "staff_project",
                    project.map(|project| {
                        work_link(
                            "staff_project",
                            "projects",
                            "Project",
                            project.id,
                            project.title,
                            project.version,
                        )
                    }),
                )
            }
            LinkedSource::Task(task) => {
                let link = match task {
                    Some(task)
                        if self
                            .task_visible(connection, scope, &candidate.id, task)
                            .await? =>
                    {
                        Some(work_link(
                            "staff_task",
                            "staff-tasks",
                            "Task",
                            task.id.clone(),
                            task.title.clone(),
                            task.version,
                        ))
                    }
                    _ => None,
                };
                ("staff_task", link)
            }
            LinkedSource::Pms(hotel) => {
                let visible = self.current.has_permission("pms.read")
                    && scope.department_id.is_none()
                    && scope
                        .branch_id
                        .as_ref()
                        .is_none_or(|branch| branch == &hotel.branch_id)
                    && (!self.purchase.pms
                        || self.purchase.release
                            && self.purchase.allows("pms.front-desk", history))
                    && pms_social_source_readable(
                        connection,
                        scope.company_id.as_str(),
                        &hotel.id,
                        scope.user_id.as_str(),
                        Some(scope.assignment_id.as_str()),
                    )
                    .await?;
                let link = if visible { hotel.link() } else { None };
                ("pms_record", link.map(|link| json!({
                    "kind": "pms_record", "available": true, "id": hotel.id, "section": link.section,
                    "sourceKind": link.source_kind, "version": link.version, "label": link.label,
                    "href": link.href, "title": link.title,
                })))
            }
        };
        Ok(visible.unwrap_or_else(|| json!({"kind": kind, "available": false})))
    }
}

fn work_link(
    kind: &str,
    path: &str,
    label: &str,
    id: String,
    title: String,
    version: i64,
) -> Value {
    let href = format!("/{path}/{}", encode_uri_component(&id));
    json!({"kind":kind, "available":true, "id":id,"title":title,
        "sourceKind":kind,"version":version,"label":label,"href":href})
}

impl SocialHomePolicy for PagePolicy<'_> {
    fn present<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        scope: &'a SocialScope,
        candidate: &'a SocialHomeCandidate,
    ) -> SocialHomePolicyFuture<'a> {
        Box::pin(async move {
            use CapabilityPurpose::{History, NewWork, SettleExisting};
            let company = scope.company_id.as_str();
            let user = scope.user_id.as_str();
            let project: Option<SocialProjectReference> = match (
                candidate.audience_kind.as_deref(),
                candidate.audience_id.as_deref(),
            ) {
                (Some("project"), Some(id)) => {
                    let Some(project) = social_project_reference(
                        connection,
                        company,
                        user,
                        id,
                        SocialProjectAccess::AudienceOrPreview,
                    )
                    .await?
                    else {
                        return Ok(None);
                    };
                    Some(project)
                }
                _ => None,
            };
            let Some(sources) = self.sources(connection, scope, candidate).await? else {
                return Ok(None);
            };
            // Classify the exact selected thread before loading its text or comments.
            // Linked body access and purchase remain tied to this Company and assignment.
            if self.direct_thread && !self.reading_origin {
                if candidate.kind != "system"
                    && !self.purchase.allows("work.social", History)
                    && sources.iter().any(LinkedSource::included)
                {
                    let source_purchase = self.purchase.enforce_source_history();
                    if self.admitted(
                        &source_purchase,
                        candidate,
                        project.is_some(),
                        &sources,
                        History,
                        false,
                    ) {
                        self.purchase = source_purchase;
                        self.source_only = true;
                    }
                }
                let normal_read = self.current.has_permission("records.read")
                    && self
                        .current
                        .modules()
                        .iter()
                        .any(|module| module == "collaboration");
                if !self.source_only && (self.require_source_only || !normal_read) {
                    return Ok(None);
                }
            }
            if !self.admitted(
                &self.purchase,
                candidate,
                project.is_some(),
                &sources,
                History,
                false,
            ) {
                return Ok(None);
            }
            let mut additions = SocialHomeAdditions {
                ai_name: self.ai_name.clone(),
                project_audience_name: project.map(|item| item.title),
                linked_sources: Vec::new(),
                files: Vec::new(),
                task: None,
                tracking_state: None,
                purchase: None,
            };
            if self.reading_origin {
                return Ok(Some(additions));
            }
            if let Some(task_id) = candidate.task_origin.as_deref()
                && let Some(task) =
                    read_social_task_source(connection, &scope.company_id, task_id, user, scope.now)
                        .await?
            {
                if self
                    .task_visible(connection, scope, &candidate.id, &task)
                    .await?
                {
                    additions.task = Some(json!({"id":task.id, "state":task.state}));
                }
                if candidate.author_id == user {
                    additions.tracking_state = Some(task.state);
                }
            }
            for source in &sources {
                additions
                    .linked_sources
                    .push(self.preview(connection, scope, candidate, source).await?);
            }
            additions.files =
                erpx_server_files::public::social_draft_uploads::read_social_home_images(
                    connection,
                    company,
                    &candidate.id,
                )
                .await?
                .into_iter()
                .map(|file| {
                    json!({
                        "post_id": file.post_id, "id": file.id, "original_name": file.original_name,
                        "media_type": file.media_type,
                    })
                })
                .collect();
            if self.purchase.work || self.purchase.projects || self.purchase.pms {
                let allow = |purpose| {
                    self.admitted(
                        &self.purchase,
                        candidate,
                        candidate.audience_kind.as_deref() == Some("project"),
                        &sources,
                        purpose,
                        true,
                    )
                };
                let new_work = allow(NewWork);
                additions.purchase = Some(json!({
                    "newWork": new_work,
                    "settleExisting": allow(SettleExisting),
                    "history": allow(History),
                    "revision": self.purchase.revision,
                    "createTask": new_work && self.purchase.release
                        && (!self.purchase.work || self.purchase.allows("work.actions", NewWork)),
                }));
            }
            Ok(Some(additions))
        })
    }
}
