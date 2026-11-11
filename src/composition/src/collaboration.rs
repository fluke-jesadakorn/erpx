//! Native Collaboration social composition.
//!
//! Account session, current IAM authority and one serializable Holding unit are
//! resolved here. Record mentions resolve through the Records owner read; no
//! Records SQL is duplicated. Messaging/inbox mounts are separate because they
//! need the Files and Work-projection ports.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_collaboration::public::social::{
    PgSocialUnit, SocialRecordFuture, SocialRecordPort, SocialScope, execute, lock_social_sources,
};
use erpx_server_collaboration::public::messaging_http::{
    MessagingAttachmentReply, MessagingChannelsData, MessagingHistoryData,
    MessagingHttpAttachmentRequest, MessagingHttpBackend, MessagingHttpCommand,
    MessagingHttpCommandRequest, MessagingHttpFuture, MessagingHttpReadReply,
    MessagingHttpReadRequest, MessagingHttpReceipt, MessagingHttpTarget, MessagingMembersData,
    MessagingOverviewData, MessagingReadKind,
};
use erpx_server_collaboration::public::messaging_native::{
    Change as RoomChange, HistoryCursor, PgMessagingUnit, Receipt as RoomReceipt, RoomView,
    lock_messaging_sources, execute as execute_room,
};
use erpx_server_collaboration::public::messaging_teams::{
    Change as TeamChange, PgMessagingTeamUnit, Receipt as TeamReceipt,
    lock_messaging_team_sources, execute as execute_team,
};
use erpx_server_collaboration::public::social_http::{
    SocialFeedData, SocialHttpBackend, SocialHttpCommandRequest, SocialHttpFuture,
    SocialHttpReadReply, SocialHttpReadRequest, SocialHttpTarget, SocialReadKind,
    SocialThreadData,
};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_records::public::record_context::{
    PgRecordContext, RecordContextActor, record_context,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope_error() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "COLLABORATION_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COLLABORATION_UNAVAILABLE",
        "The Collaboration service is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct CollaborationSources;

impl HoldingWorkSourcePreparation for CollaborationSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct SocialConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl SocialConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

/// Records-backed mention resolver. It runs the owner contract read on the same
/// unit, so visibility and link admission stay in the Records owner.
struct SocialRecordsPort {
    actor: RecordContextActor,
    now: i64,
}

impl SocialRecordPort for SocialRecordsPort {
    fn resolve_record<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        module: &'a str,
        id: &'a str,
    ) -> SocialRecordFuture<'a> {
        Box::pin(async move {
            let mut repository = PgRecordContext::new(connection);
            let mut guard = crate::records::RecordsContextGuardImpl;
            let document = record_context(
                &mut repository,
                &mut guard,
                &self.actor,
                module,
                id,
                self.now,
            )
            .await
            .map_err(business)?;
            Ok(Some(
                document
                    .get("record")
                    .and_then(|record| record.get("title"))
                    .and_then(|title| title.as_str())
                    .unwrap_or(id)
                    .to_owned(),
            ))
        })
    }
}

async fn display_name(
    connection: &mut PgConnection,
    company: &CompanyId,
    user_id: &str,
) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT COALESCE(NULLIF(preferred_name,''),legal_name) FROM hr_people \
         WHERE company_id=$1 AND user_id=$2 AND status='active' AND merged_into_id IS NULL LIMIT 1",
    )
    .bind(company.as_str())
    .bind(user_id)
    .fetch_optional(&mut *connection)
    .await
    .ok()
    .flatten()
    .unwrap_or_else(|| user_id.to_owned())
}

fn social_scope(
    current: &AuthorizedWorkContext,
    company: &CompanyId,
    name: String,
    now: i64,
    use_assignment_scope: bool,
) -> Result<SocialScope, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    let (branch, department) = if use_assignment_scope {
        match assignment.scope_kind {
            WorkScopeKind::Holding => (None, None),
            WorkScopeKind::Branch => (
                assignment.scope_node_id.as_ref().map(|id| id.as_str().to_owned()),
                None,
            ),
            WorkScopeKind::Department => {
                let department = assignment
                    .scope_node_id
                    .as_ref()
                    .map(|id| id.as_str().to_owned());
                (
                    workspace.branch_id.as_ref().map(|id| id.as_str().to_owned()),
                    department,
                )
            }
        }
    } else {
        (
            workspace.branch_id.as_ref().map(|id| id.as_str().to_owned()),
            workspace
                .department_id
                .as_ref()
                .map(|id| id.as_str().to_owned()),
        )
    };
    let can_share_company = assignment.scope_kind == WorkScopeKind::Holding;
    let can_moderate = current.has_permission("records.review");
    let scope = SocialScope::new(
        company.as_str(),
        current.user_id().as_str(),
        assignment.id.as_str(),
        name,
        now,
    )?
    .branch(branch.as_deref())?
    .department(department.as_deref())?
    .assignment_version(assignment.version)
    .share_company(can_share_company)
    .moderate(can_moderate);
    Ok(scope)
}

pub struct SocialService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: SocialConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> SocialService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: SocialConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
        }
    }

    pub fn company(&self) -> &CompanyId {
        self.configuration.company()
    }

    pub async fn read(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        kind: SocialReadKind,
    ) -> Result<SocialHttpReadReply, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let expected = company.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || {
                    let mut request = selection;
                    if request.company_id != expected {
                        return Err(scope_error());
                    }
                    request.mutation = false;
                    Ok(request)
                },
                WorkAuthorizationRequest {
                    permission: "records.read".into(),
                    module: Some("collaboration".into()),
                    destination: "feed".into(),
                    api: true,
                },
                CollaborationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let kind = kind.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_error().into());
                        }
                        let now = clock.now().get();
                        let name = display_name(connection, &company, current.user_id().as_str())
                            .await;
                        let actor = crate::records::records_actor(current)?;
                        let scope = social_scope(current, &company, name, now, false)?;
                        let mut records = SocialRecordsPort { actor, now };
                        let mut unit =
                            PgSocialUnit::read_only(connection, scope, &mut records);
                        Ok(match kind {
                            SocialReadKind::Feed {
                                filter,
                                cursor,
                                limit,
                            } => {
                                let (items, has_more) =
                                    unit.feed_page(filter, cursor.as_ref(), limit).await?;
                                let next_cursor = if has_more {
                                    items.last().map(|post| {
                                        erpx_server_collaboration::public::social::FeedCursor {
                                            at: post.created_at,
                                            id: post.id.clone(),
                                        }
                                        .encode()
                                    })
                                } else {
                                    None
                                };
                                SocialHttpReadReply::Feed(Box::new(SocialFeedData {
                                    items,
                                    next_cursor,
                                    has_more,
                                }))
                            }
                            SocialReadKind::Thread { post_id } => {
                                let posts = unit.thread(&post_id).await?;
                                SocialHttpReadReply::Thread(Box::new(SocialThreadData { posts }))
                            }
                        })
                    })
                },
            )
            .await
    }

    pub async fn command(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        change: erpx_server_collaboration::public::social::Change,
        key: erpx_server_shared::public::CommandKey,
        now_body: serde_json::Value,
    ) -> Result<erpx_server_collaboration::public::social::Receipt, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let expected = company.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || {
                    let mut request = selection;
                    if request.company_id != expected {
                        return Err(scope_error());
                    }
                    request.mutation = true;
                    Ok(request)
                },
                WorkAuthorizationRequest {
                    permission: "records.write".into(),
                    module: Some("collaboration".into()),
                    destination: "feed".into(),
                    api: true,
                },
                CollaborationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let change = change.clone();
                    let key = key.clone();
                    let _ = now_body.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_error().into());
                        }
                        lock_social_sources(connection).await?;
                        let now = clock.now().get();
                        let name = display_name(connection, &company, current.user_id().as_str())
                            .await;
                        let actor = crate::records::records_actor(current)?;
                        let scope = social_scope(current, &company, name, now, true)?;
                        let mut records = SocialRecordsPort { actor, now };
                        let mut unit = PgSocialUnit::new(connection, scope, key.clone(), &mut records);
                        let receipt = execute(&mut unit, &key, change).await?;
                        Ok(receipt)
                    })
                },
            )
            .await
    }
}

pub struct SocialBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<SocialService<C>>,
}

impl<C: Clock + Send + Sync + 'static> SocialBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<SocialService<C>>,
    ) -> Self {
        Self {
            policy,
            scopes,
            service,
        }
    }

    fn context(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found());
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.service.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: false,
            },
        ))
    }
}

impl<C: Clock + Send + Sync + 'static> SocialHttpBackend for SocialBackend<C> {
    fn read<'a>(
        &'a self,
        request: SocialHttpReadRequest<'a>,
    ) -> SocialHttpFuture<'a, SocialHttpReadReply> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .read(request.host, proof, selection, request.kind)
                .await
                .map_err(business)
        })
    }

    fn command<'a>(
        &'a self,
        request: SocialHttpCommandRequest<'a>,
    ) -> SocialHttpFuture<'a, erpx_server_collaboration::public::social::Receipt> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .command(
                    request.host,
                    proof,
                    selection,
                    request.change,
                    request.key,
                    request.input,
                )
                .await
                .map_err(business)
        })
    }
}

pub fn social_target(
    host: String,
    holding_slug: String,
    origin: String,
    backend: Arc<dyn SocialHttpBackend>,
) -> SocialHttpTarget {
    SocialHttpTarget {
        host,
        holding_slug,
        origin,
        backend,
    }
}

fn messaging_scope(
    current: &AuthorizedWorkContext,
    company: &CompanyId,
    name: String,
    now: i64,
) -> Result<erpx_server_collaboration::public::messaging_native::MessagingScope, BusinessError> {
    erpx_server_collaboration::public::messaging_native::MessagingScope::new(
        company.as_str(),
        current.user_id().as_str(),
        current.selection().assignment().id.as_str(),
        name,
        now,
    )
}

fn room_view(room: &erpx_server_collaboration::public::messaging_native::RoomSnapshot) -> RoomView {
    RoomView {
        id: room.id.clone(),
        kind: room.kind.as_str().to_owned(),
        name: room.name.clone(),
        owner_id: room.owner_id.clone(),
        version: room.version,
        last_seq: room.last_seq,
        unread: 0,
        members: room.members.clone(),
        last_message_at: None,
    }
}

fn attachment_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FILES_PORT_UNAVAILABLE",
        "Messaging attachments are unavailable.",
    )
}

pub struct MessagingService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: SocialConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> MessagingService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: SocialConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
        }
    }

    pub fn company(&self) -> &CompanyId {
        self.configuration.company()
    }

    pub async fn read(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        kind: MessagingReadKind,
    ) -> Result<MessagingHttpReadReply, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let expected = company.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || {
                    let mut request = selection;
                    if request.company_id != expected {
                        return Err(scope_error());
                    }
                    request.mutation = false;
                    Ok(request)
                },
                WorkAuthorizationRequest {
                    permission: "records.read".into(),
                    module: Some("collaboration".into()),
                    destination: "messaging".into(),
                    api: true,
                },
                CollaborationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let kind = kind.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_error().into());
                        }
                        let now = clock.now().get();
                        let name = display_name(connection, &company, current.user_id().as_str())
                            .await;
                        let scope = messaging_scope(current, &company, name, now)?;
                        Ok(match kind {
                            MessagingReadKind::Overview => {
                                let mut rooms = PgMessagingUnit::read_only(connection, scope.clone());
                                let rooms = rooms.overview().await?;
                                let mut teams =
                                    PgMessagingTeamUnit::read_only(connection, scope);
                                let teams = teams.teams().await?;
                                MessagingHttpReadReply::Overview(Box::new(
                                    MessagingOverviewData { teams, rooms },
                                ))
                            }
                            MessagingReadKind::History {
                                room_id,
                                before,
                                after,
                                limit,
                            } => {
                                let mut unit =
                                    PgMessagingUnit::read_only(connection, scope);
                                let room = unit.room(&room_id).await?.map(|room| room_view(&room));
                                let messages =
                                    unit.history(&room_id, before, after, limit).await?;
                                let has_more = messages.len() >= limit;
                                let room = room.ok_or_else(|| {
                                    BusinessError::new(
                                        FailureKind::NotFound,
                                        "NOT_FOUND",
                                        "Not found.",
                                    )
                                })?;
                                MessagingHttpReadReply::History(Box::new(MessagingHistoryData {
                                    room,
                                    messages,
                                    has_more,
                                }))
                            }
                            MessagingReadKind::Members { id, is_team } => {
                                let members = if is_team {
                                    let mut teams =
                                        PgMessagingTeamUnit::read_only(connection, scope);
                                    teams.members(&id).await?
                                } else {
                                    let mut unit =
                                        PgMessagingUnit::read_only(connection, scope);
                                    unit.room_members(&id).await?
                                };
                                MessagingHttpReadReply::Members(Box::new(MessagingMembersData {
                                    id,
                                    is_team,
                                    members,
                                }))
                            }
                            MessagingReadKind::TeamChannels { team_id } => {
                                let mut teams =
                                    PgMessagingTeamUnit::read_only(connection, scope);
                                let team = teams.team(&team_id).await?.ok_or_else(|| {
                                    BusinessError::new(
                                        FailureKind::NotFound,
                                        "NOT_FOUND",
                                        "Not found.",
                                    )
                                })?;
                                let channels = teams.channels(&team_id, 100).await?;
                                MessagingHttpReadReply::Channels(Box::new(
                                    MessagingChannelsData { team, channels },
                                ))
                            }
                        })
                    })
                },
            )
            .await
    }

    pub async fn command(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        command: MessagingHttpCommand,
        key: erpx_server_shared::public::CommandKey,
    ) -> Result<MessagingHttpReceipt, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let expected = company.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || {
                    let mut request = selection;
                    if request.company_id != expected {
                        return Err(scope_error());
                    }
                    request.mutation = true;
                    Ok(request)
                },
                WorkAuthorizationRequest {
                    permission: "records.write".into(),
                    module: Some("collaboration".into()),
                    destination: "messaging".into(),
                    api: true,
                },
                CollaborationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let key = key.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_error().into());
                        }
                        let now = clock.now().get();
                        let name = display_name(connection, &company, current.user_id().as_str())
                            .await;
                        let scope = messaging_scope(current, &company, name, now)?;
                        Ok(match command {
                            MessagingHttpCommand::Room(change) => {
                                lock_messaging_sources(connection).await?;
                                let mut unit =
                                    PgMessagingUnit::new(connection, scope, key.clone());
                                let receipt = execute_room(&mut unit, &key, change).await?;
                                MessagingHttpReceipt::Room { receipt }
                            }
                            MessagingHttpCommand::Team(change) => {
                                lock_messaging_team_sources(connection).await?;
                                let mut unit =
                                    PgMessagingTeamUnit::new(connection, scope, key.clone());
                                let receipt = execute_team(&mut unit, &key, change).await?;
                                MessagingHttpReceipt::Team { receipt }
                            }
                        })
                    })
                },
            )
            .await
    }
}

pub struct MessagingBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<MessagingService<C>>,
}

impl<C: Clock + Send + Sync + 'static> MessagingBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<MessagingService<C>>,
    ) -> Self {
        Self {
            policy,
            scopes,
            service,
        }
    }

    fn context(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found());
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.service.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: false,
            },
        ))
    }
}

impl<C: Clock + Send + Sync + 'static> MessagingHttpBackend for MessagingBackend<C> {
    fn read<'a>(
        &'a self,
        request: MessagingHttpReadRequest<'a>,
    ) -> MessagingHttpFuture<'a, MessagingHttpReadReply> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .read(request.host, proof, selection, request.kind)
                .await
                .map_err(business)
        })
    }

    fn command<'a>(
        &'a self,
        request: MessagingHttpCommandRequest<'a>,
    ) -> MessagingHttpFuture<'a, MessagingHttpReceipt> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .command(
                    request.host,
                    proof,
                    selection,
                    request.command,
                    request.key,
                )
                .await
                .map_err(business)
        })
    }

    fn attachment<'a>(
        &'a self,
        _request: MessagingHttpAttachmentRequest<'a>,
        _files: &'a dyn erpx_server_collaboration::public::messaging_http::MessagingFilesPort,
    ) -> MessagingHttpFuture<'a, MessagingAttachmentReply> {
        Box::pin(async { Err(attachment_unavailable()) })
    }
}

fn inbox_scope(
    current: &AuthorizedWorkContext,
    company: &CompanyId,
    now: i64,
) -> Result<erpx_server_collaboration::public::inbox::InboxScope, BusinessError> {
    erpx_server_collaboration::public::inbox::InboxScope::new(
        company.as_str(),
        current.user_id().as_str(),
        current.selection().assignment().id.as_str(),
        now,
    )
    .map(|scope| scope.assignment_version(current.selection().assignment().version))
}

pub struct InboxService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: SocialConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> InboxService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: SocialConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
        }
    }

    pub fn company(&self) -> &CompanyId {
        self.configuration.company()
    }

    pub async fn read(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        kind: erpx_server_collaboration::public::inbox::InboxReadKind,
    ) -> Result<
        erpx_server_collaboration::public::inbox::InboxHttpReadReply,
        Error,
    > {
        use erpx_server_collaboration::public::inbox::{
            InboxHttpReadReply, InboxNotificationQuery, InboxReadKind, PgInboxTargets,
            PgInboxUnit,
        };
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let expected = company.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || {
                    let mut request = selection;
                    if request.company_id != expected {
                        return Err(scope_error());
                    }
                    request.mutation = false;
                    Ok(request)
                },
                WorkAuthorizationRequest {
                    permission: "records.read".into(),
                    module: Some("collaboration".into()),
                    destination: "inbox".into(),
                    api: true,
                },
                CollaborationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let kind = kind.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_error().into());
                        }
                        let now = clock.now().get();
                        let selected = current.selection();
                        let workspace = selected.workspace();
                        let scope = inbox_scope(current, &company, now)?;
                        let mut targets = PgInboxTargets::new(
                            workspace.branch_id.as_ref().map(|id| id.as_str()),
                            workspace.department_id.as_ref().map(|id| id.as_str()),
                        )
                        .assignment(Some(selected.assignment().id.as_str()));
                        let mut unit = PgInboxUnit::new(connection, scope, &mut targets);
                        Ok(match kind {
                            InboxReadKind::Summary => {
                                InboxHttpReadReply::Summary(Box::new(unit.summary().await?))
                            }
                            InboxReadKind::Notifications {
                                view,
                                category,
                                search,
                                cursor,
                                limit,
                            } => InboxHttpReadReply::Page(Box::new(
                                unit.notifications(InboxNotificationQuery {
                                    view,
                                    category,
                                    search,
                                    cursor,
                                    limit,
                                })
                                .await?,
                            )),
                            InboxReadKind::People { query } => {
                                InboxHttpReadReply::People(
                                    serde_json::to_value(unit.people(&query).await?)
                                        .map_err(|_| unavailable())?,
                                )
                            }
                            InboxReadKind::Notices => {
                                InboxHttpReadReply::Notices(
                                    serde_json::to_value(unit.notices().await?)
                                        .map_err(|_| unavailable())?,
                                )
                            }
                            InboxReadKind::Notice { id } => {
                                InboxHttpReadReply::Notice(
                                    serde_json::to_value(unit.notice(&id).await?)
                                        .map_err(|_| unavailable())?,
                                )
                            }
                        })
                    })
                },
            )
            .await
    }
}

pub struct InboxBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<InboxService<C>>,
}

impl<C: Clock + Send + Sync + 'static> InboxBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<InboxService<C>>,
    ) -> Self {
        Self {
            policy,
            scopes,
            service,
        }
    }

    fn context(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found());
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.service.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: false,
            },
        ))
    }
}

impl<C: Clock + Send + Sync + 'static>
    erpx_server_collaboration::public::inbox::InboxHttpBackend for InboxBackend<C>
{
    fn read<'a>(
        &'a self,
        request: erpx_server_collaboration::public::inbox::InboxHttpReadRequest<'a>,
    ) -> erpx_server_collaboration::public::inbox::InboxHttpFuture<
        'a,
        erpx_server_collaboration::public::inbox::InboxHttpReadReply,
    > {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .read(request.host, proof, selection, request.kind)
                .await
                .map_err(business)
        })
    }
}
