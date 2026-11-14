//! Native Collaboration social composition.
//!
//! Account session, current IAM authority and one serializable Holding unit are
//! resolved here. Record mentions resolve through the Records owner read; no
//! Records SQL is duplicated. Messaging/inbox mounts are separate because they
//! need the Files and Work-projection ports.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_collaboration::public::social::{
    PgSocialUnit, SocialRecordFuture, SocialRecordPort, SocialScope, SocialUploadFilesFuture,
    SocialUploadFilesPort, execute, lock_social_sources,
};
use erpx_server_collaboration::public::messaging_http::{
    MessagingAttachmentKind, MessagingAttachmentReply, MessagingChannelsData,
    MessagingFileBinding, MessagingFileContent, MessagingFileDiscardReceipt,
    MessagingFileDiscardRequest, MessagingFilesBindRequest, MessagingFilesContentRequest,
    MessagingFilesPort, MessagingHistoryData, MessagingHttpAttachmentRequest, MessagingHttpBackend,
    MessagingHttpCommand, MessagingHttpCommandRequest, MessagingHttpFuture, MessagingHttpReadReply,
    MessagingHttpReadRequest, MessagingHttpReceipt, MessagingHttpTarget, MessagingMembersData,
    MessagingOverviewData, MessagingReadKind,
};
use erpx_server_collaboration::public::messaging_native::{
    Change as RoomChange, HistoryCursor, MessagingActor, PgMessagingUnit, Receipt as RoomReceipt,
    RoomView, lock_messaging_sources, execute as execute_room,
};
use erpx_server_collaboration::public::messaging_file_discard::{
    MessagingFileDiscardPort, MessagingFileWithdrawRequest, discard_file,
};
use erpx_server_collaboration::public::messaging_teams::{
    Change as TeamChange, PgMessagingTeamUnit, Receipt as TeamReceipt,
    lock_messaging_team_sources, execute as execute_team,
};
use erpx_server_collaboration::public::social_http::{
    SocialCommentsData, SocialFeedData, SocialHttpBackend, SocialHttpCommentsRequest,
    SocialHttpCommandRequest, SocialHttpFuture, SocialHttpReadReply, SocialHttpReadRequest,
    SocialHttpRevisionsRequest, SocialHttpTarget, SocialReadKind, SocialThreadData,
};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::{parse_inbound_account_request, unique_request_header},
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_files::public::messaging_files::{
    MessagingFilesActor, MessagingFilesAudience, MessagingFilesFuture,
};
use erpx_server_files::public::attachment_command::{
    AttachmentWithdrawAuthority, WithdrawAttachmentRequest, WithdrawTarget, withdraw_attachment,
};
use erpx_server_commercial::public::purchase_command_boundary::{
    PurchaseCommandBoundary, PurchaseCommandScope,
};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, current_package_catalogue,
};
use erpx_server_identity_access::public::selected_command_authority::require_selected_actor_current;
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

/// Write preparation: Collaboration rows plus every owner record-source table
/// are fenced before the first selected IAM snapshot.
#[derive(Clone, Copy)]
struct CollaborationWorkSources;

impl HoldingWorkSourcePreparation for CollaborationWorkSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            erpx_server_collaboration::public::collaboration_work::lock_collaboration_work_sources(c)
                .await?;
            erpx_server_commerce::public::record_source::lock_record_sources(c).await?;
            erpx_server_support::public::record_source::lock_record_sources(c).await?;
            erpx_server_maintenance::public::record_source::lock_record_sources(c).await?;
            erpx_server_operations::public::record_source::lock_record_sources(c).await?;
            erpx_server_pos::public::record_source::lock_record_sources(c).await?;
            erpx_server_pms::public::record_source::lock_record_sources(c).await?;
            erpx_server_assets::public::record_source::lock_record_sources(c).await?;
            Ok(())
        })
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

/// Files owner port for canceled Social upload drafts. Files owns the
/// attachment revocation and upload-intent expiry on the caller's retained
/// serializable connection; Collaboration never writes those tables.
struct SocialUploadFilesPortImpl;

impl SocialUploadFilesPort for SocialUploadFilesPortImpl {
    fn revoke_draft_uploads<'a>(
        &'a mut self,
        connection: &'a mut sqlx::PgConnection,
        draft_id: &'a str,
        now: i64,
    ) -> SocialUploadFilesFuture<'a> {
        Box::pin(async move {
            erpx_server_files::public::social_draft_uploads::revoke_social_draft_uploads(
                connection,
                erpx_server_files::public::social_draft_uploads::SocialDraftUploadRevokeRequest {
                    draft_id,
                    now,
                },
            )
            .await
            .map_err(business)
        })
    }
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
    .moderate(can_moderate)
    .hide_posts(assignment.role == "holding_owner");
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

    pub async fn comments_page(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        post_id: String,
        cursor: Option<erpx_server_collaboration::public::social::FeedCursor>,
        limit: usize,
    ) -> Result<SocialCommentsData, Error> {
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
                    let post_id = post_id.clone();
                    let cursor = cursor.clone();
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
                        let (items, next) = unit
                            .comments_page(&post_id, cursor.as_ref(), limit, 20)
                            .await?;
                        Ok(SocialCommentsData::new(items, next))
                    })
                },
            )
            .await
    }

    pub async fn revisions(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        post_id: String,
    ) -> Result<Vec<erpx_server_collaboration::public::social::RevisionRow>, Error> {
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
                    let post_id = post_id.clone();
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
                        Ok(unit.revisions(&post_id).await?)
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
                        let mut uploads = SocialUploadFilesPortImpl;
                        let mut unit = PgSocialUnit::new(connection, scope, key.clone(), &mut records)
                            .with_upload_files(&mut uploads);
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

    fn comments_page<'a>(
        &'a self,
        request: SocialHttpCommentsRequest<'a>,
    ) -> SocialHttpFuture<'a, SocialCommentsData> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .comments_page(
                    request.host,
                    proof,
                    selection,
                    request.post_id.to_owned(),
                    request.cursor,
                    request.limit,
                )
                .await
                .map_err(business)
        })
    }

    fn revisions<'a>(
        &'a self,
        request: SocialHttpRevisionsRequest<'a>,
    ) -> SocialHttpFuture<'a, Vec<erpx_server_collaboration::public::social::RevisionRow>> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .revisions(
                    request.host,
                    proof,
                    selection,
                    request.post_id.to_owned(),
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

async fn messaging_member_names(
    control: &erpx_server_runtime::public::postgres::DatabasePool,
    ids: &[String],
) -> Result<Vec<serde_json::Value>, BusinessError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut connection = control
        .raw()
        .acquire()
        .await
        .map_err(|_| unavailable())?;
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id,name,email FROM \"user\" WHERE id = ANY($1::text[]) ORDER BY name,id",
    )
    .bind(ids)
    .fetch_all(&mut *connection)
    .await
    .map_err(|_| unavailable())?;
    Ok(rows
        .into_iter()
        .map(|(id, name, email)| serde_json::json!({"id": id, "name": name, "email": email}))
        .collect())
}

pub struct MessagingService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: SocialConfiguration,
    control: erpx_server_runtime::public::postgres::DatabasePool,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> MessagingService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: SocialConfiguration,
        control: erpx_server_runtime::public::postgres::DatabasePool,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            control,
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
        let control = self.control.clone();
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
                    let control = control.clone();
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
                                let ids: Vec<String> =
                                    messages.iter().map(|message| message.id.clone()).collect();
                                let files = unit.files_for_messages(&ids).await?;
                                let messages: Vec<serde_json::Value> = messages
                                    .iter()
                                    .map(|message| {
                                        let mut value = serde_json::to_value(message)
                                            .map_err(|_| unavailable())?;
                                        let row_files: Vec<serde_json::Value> = files
                                            .iter()
                                            .filter(|file| {
                                                file.get("message_id")
                                                    .and_then(|id| id.as_str())
                                                    == Some(message.id.as_str())
                                            })
                                            .cloned()
                                            .collect();
                                        if let Some(object) = value.as_object_mut() {
                                            object.insert(
                                                "files".into(),
                                                serde_json::Value::Array(row_files),
                                            );
                                        }
                                        Ok(value)
                                    })
                                    .collect::<Result<Vec<_>, BusinessError>>()
                                    .map_err(Error::Business)?;
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
                                let (ids, detail) = if is_team {
                                    let mut teams =
                                        PgMessagingTeamUnit::read_only(connection, scope);
                                    let team = teams.team(&id).await?.ok_or_else(|| {
                                        BusinessError::new(
                                            FailureKind::NotFound,
                                            "NOT_FOUND",
                                            "Not found.",
                                        )
                                    })?;
                                    let ids = teams.members(&id).await?;
                                    let detail = serde_json::json!({
                                        "id": team.id,
                                        "companyId": team.company_id,
                                        "name": team.name,
                                        "ownerId": team.owner_id,
                                        "version": team.version,
                                        "createdAt": team.created_at,
                                        "kind": "team",
                                    });
                                    (ids, detail)
                                } else {
                                    let mut unit =
                                        PgMessagingUnit::read_only(connection, scope);
                                    let room = unit.room(&id).await?.ok_or_else(|| {
                                        BusinessError::new(
                                            FailureKind::NotFound,
                                            "NOT_FOUND",
                                            "Not found.",
                                        )
                                    })?;
                                    let ids = unit.room_members(&id).await?;
                                    let detail = serde_json::json!({
                                        "id": room.id,
                                        "companyId": room.company_id,
                                        "kind": room.kind.as_str(),
                                        "name": room.name,
                                        "ownerId": room.owner_id,
                                        "version": room.version,
                                        "lastSeq": room.last_seq,
                                        "createdAt": room.created_at,
                                    });
                                    (ids, detail)
                                };
                                let members = messaging_member_names(&control, &ids).await?;
                                MessagingHttpReadReply::Members(Box::new(MessagingMembersData {
                                    id,
                                    is_team,
                                    members,
                                    detail,
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

    pub async fn discard(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        file_id: String,
        reason: String,
        key: erpx_server_shared::public::CommandKey,
    ) -> Result<MessagingFileDiscardReceipt, Error> {
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
                    if request.company_id != expected || !request.mutation {
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
                    let file_id = file_id.clone();
                    let reason = reason.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_error().into());
                        }
                        current.require_permission("records.write")?;
                        current.require_permission("records.read")?;
                        let now = clock.now().get();
                        let name = display_name(connection, &company, current.user_id().as_str())
                            .await;
                        let actor = MessagingActor {
                            user_id: current.user_id().as_str().to_owned(),
                            display_name: name,
                            company_id: company.as_str().to_owned(),
                        };
                        // Retained order: Company package fence before the
                        // Messaging/owner source locks.
                        let mut purchase = begin_messaging_file_discard_purchase(
                            connection,
                            &holding,
                            &company,
                            key.as_str(),
                        )
                        .await?;
                        lock_messaging_sources(connection).await?;
                        sqlx::query(
                            "LOCK TABLE messaging_files IN SHARE ROW EXCLUSIVE MODE",
                        )
                        .execute(&mut *connection)
                        .await?;
                        let authority = MessageFileWithdrawAuthority {
                            company_id: company.clone(),
                            current,
                        };
                        let port = MessageFileDiscardFilesPort {
                            authority: &authority,
                        };
                        let receipt = discard_file(
                            connection,
                            &actor,
                            current.selection().assignment().id.as_str(),
                            key.as_str(),
                            &file_id,
                            &reason,
                            now,
                            &port,
                        )
                        .await?;
                        if let Some((mut boundary, catalogue)) = purchase.take() {
                            boundary.complete(connection, &holding, &catalogue).await?;
                        }
                        Ok(MessagingFileDiscardReceipt {
                            id: receipt.id,
                            state: receipt.state,
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

    fn mutation_context(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found());
        }
        let company =
            unique_request_header(headers, "x-erpx-context-company")?.ok_or_else(scope_error)?;
        if company != self.service.company().as_str() {
            return Err(scope_error());
        }
        let assignment = unique_request_header(headers, "x-erpx-context-assignment")?
            .ok_or_else(scope_error)?;
        let revision: i64 = unique_request_header(headers, "x-erpx-context-revision")?
            .ok_or_else(scope_error)?
            .parse()
            .map_err(|_| scope_error())?;
        let assignment_version: i64 =
            unique_request_header(headers, "x-erpx-context-assignment-version")?
                .ok_or_else(scope_error)?
                .parse()
                .map_err(|_| scope_error())?;
        if !(1..=9_007_199_254_740_991).contains(&revision)
            || !(1..=9_007_199_254_740_991).contains(&assignment_version)
        {
            return Err(scope_error());
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.service.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: Some(
                    erpx_server_identity_access::public::work_context::ExpectedWorkSelection {
                        revision,
                        assignment_id: erpx_server_shared::public::AssignmentId::new(assignment)?,
                        assignment_version,
                    },
                ),
                mutation: true,
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
        request: MessagingHttpAttachmentRequest<'a>,
        files: &'a dyn MessagingFilesPort,
    ) -> MessagingHttpFuture<'a, MessagingAttachmentReply> {
        Box::pin(async move {
            let MessagingHttpAttachmentRequest {
                host,
                holding_slug,
                headers,
                kind,
                ..
            } = request;
            Ok(match kind {
                MessagingAttachmentKind::Bind {
                    room_id,
                    attachment_set_ids,
                    message_id,
                } => MessagingAttachmentReply::Bound(
                    files
                        .bind(MessagingFilesBindRequest {
                            host,
                            holding_slug,
                            headers,
                            room_id,
                            attachment_set_ids,
                            message_id,
                        })
                        .await?,
                ),
                MessagingAttachmentKind::Content { file_id, variant } => {
                    MessagingAttachmentReply::Content(
                        files
                            .content(MessagingFilesContentRequest {
                                host,
                                holding_slug,
                                headers,
                                file_id,
                                variant,
                            })
                            .await?,
                    )
                }
            })
        })
    }

    fn discard<'a>(
        &'a self,
        request: MessagingFileDiscardRequest<'a>,
    ) -> MessagingHttpFuture<'a, MessagingFileDiscardReceipt> {
        Box::pin(async move {
            let (proof, selection) = self.mutation_context(request.headers)?;
            self.service
                .discard(
                    request.host,
                    proof,
                    selection,
                    request.file_id.to_owned(),
                    request.reason.to_owned(),
                    request.key,
                )
                .await
                .map_err(business)
        })
    }
}

/// Composition-owned Messaging attachment audience. It re-resolves the exact
/// current Account/Holding session through the same authorizer as Messaging
/// reads, then rechecks current conversation membership on the caller's
/// existing Holding unit. It never commits that unit.
pub struct CompositionMessagingFilesAudience {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    authorizer: Arc<HoldingWorkAuthorizer>,
    holding: HoldingId,
    company: CompanyId,
}

impl CompositionMessagingFilesAudience {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        authorizer: Arc<HoldingWorkAuthorizer>,
        holding: HoldingId,
        company: CompanyId,
    ) -> Self {
        Self {
            policy,
            scopes,
            authorizer,
            holding,
            company,
        }
    }
}

impl MessagingFilesAudience for CompositionMessagingFilesAudience {
    fn current<'a>(
        &'a self,
        _connection: &'a mut PgConnection,
        host: &'a str,
        _holding_slug: &'a str,
        headers: &'a HeaderMap,
    ) -> MessagingFilesFuture<'a, MessagingFilesActor> {
        Box::pin(async move {
            let inbound = parse_inbound_account_request(&self.policy, headers)?;
            if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
                return Err(not_found());
            }
            let (branch_slug, department_slug) = self.scopes.slugs(headers)?;
            let expected = self.company.clone();
            let company = self.company.clone();
            let holding = self.holding.clone();
            self.authorizer
                .execute_with_sources_deferred(
                    host,
                    inbound.into_proof(),
                    move || {
                        Ok(HoldingWorkspaceRequest {
                            company_id: expected,
                            branch_slug,
                            department_slug,
                            expected: None,
                            mutation: false,
                        })
                    },
                    WorkAuthorizationRequest {
                        permission: "records.read".into(),
                        module: Some("collaboration".into()),
                        destination: "messaging".into(),
                        api: true,
                    },
                    CollaborationSources,
                    move |_connection, current| {
                        let company = company.clone();
                        let holding = holding.clone();
                        Box::pin(async move {
                            if current.holding_id() != &holding
                                || current.selection().workspace().company_id != company
                            {
                                return Err(scope_error().into());
                            }
                            Ok(MessagingFilesActor {
                                company_id: company,
                                user_id: current.user_id().as_str().to_owned(),
                            })
                        })
                    },
                )
                .await
                .map_err(business)
        })
    }

    fn require_room<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        actor: &'a MessagingFilesActor,
        room_id: &'a str,
    ) -> MessagingFilesFuture<'a, ()> {
        Box::pin(async move {
            let member: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM messaging_room_members m \
                 JOIN messaging_rooms r ON r.id=m.room_id AND r.company_id=$3 \
                 WHERE m.room_id=$1 AND m.user_id=$2)",
            )
            .bind(room_id)
            .bind(&actor.user_id)
            .bind(actor.company_id.as_str())
            .fetch_one(&mut *connection)
            .await
            .map_err(|_| unavailable())?;
            if !member {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "MESSAGING_ROOM_FORBIDDEN",
                    "Choose a conversation you belong to.",
                ));
            }
            Ok(())
        })
    }
}

/// Files owner authority for a Messaging unsent-file discard. The current
/// Account/Holding/Company scope and the `collaboration` module are required,
/// and the exact `messaging_room` target must stay in this Company. Room
/// membership is rechecked by the Messaging unit before and after the Files
/// write; this port never mints a Customer, Account or File permission.
struct MessageFileWithdrawAuthority<'a> {
    company_id: CompanyId,
    current: &'a AuthorizedWorkContext,
}

impl MessageFileWithdrawAuthority<'_> {
    fn require(&self, target: &WithdrawTarget<'_>) -> Result<(), BusinessError> {
        if target.company_id != self.company_id.as_str()
            || target.target_type != "messaging_room"
        {
            return Err(scope_error());
        }
        self.current
            .require_permission("records.write")
            .map_err(|_| {
                BusinessError::new(
                    FailureKind::Forbidden,
                    "ATTACHMENT_FORBIDDEN",
                    "Your current assignment cannot discard this file.",
                )
            })?;
        self.current
            .require_permission("records.read")
            .map_err(|_| {
                BusinessError::new(
                    FailureKind::Forbidden,
                    "ATTACHMENT_FORBIDDEN",
                    "Your current assignment cannot discard this file.",
                )
            })?;
        if !self
            .current
            .modules()
            .iter()
            .any(|value| value == "collaboration")
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ATTACHMENT_FORBIDDEN",
                "Your current assignment cannot discard this file.",
            ));
        }
        Ok(())
    }
}

impl AttachmentWithdrawAuthority for MessageFileWithdrawAuthority<'_> {
    fn authorize_withdraw<'c>(
        &'c self,
        _connection: &'c mut PgConnection,
        target: &'c WithdrawTarget<'c>,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            self.require(target)?;
            Ok(())
        })
    }

    fn revalidate_withdraw<'c>(
        &'c self,
        connection: &'c mut PgConnection,
        target: &'c WithdrawTarget<'c>,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            self.require(target).map_err(Error::Business)?;
            let now = self.current.expires_at();
            let sampled = sqlx::query_scalar::<_, i64>(
                "SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint",
            )
            .fetch_one(&mut *connection)
            .await?;
            if sampled >= now {
                return Err(Error::Business(BusinessError::new(
                    FailureKind::Conflict,
                    "ATTACHMENT_WITHDRAW_AUTHORITY_CHANGED",
                    "The selected role or file record changed. Reload and retry.",
                )));
            }
            require_selected_actor_current(connection, self.current, sampled).await?;
            Ok(())
        })
    }
}

/// In-process Files owner port for the Messaging unit. It calls the Files
/// generic remove command on the caller's connection; both owners commit or
/// roll back together.
struct MessageFileDiscardFilesPort<'a> {
    authority: &'a MessageFileWithdrawAuthority<'a>,
}

impl MessagingFileDiscardPort for MessageFileDiscardFilesPort<'_> {
    fn withdraw<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        request: MessagingFileWithdrawRequest<'a>,
    ) -> erpx_server_collaboration::public::messaging_file_discard::MessagingFileDiscardFuture<'a, ()>
    {
        Box::pin(async move {
            withdraw_attachment(
                connection,
                self.authority,
                WithdrawAttachmentRequest {
                    actor_id: request.actor_id,
                    company_id: request.company_id,
                    link_id: request.link_id,
                    reason: request.reason,
                    command_key: request.command_key,
                    now: request.now,
                },
            )
            .await
            .map(|_| ())
            .map_err(business)
        })
    }
}

fn messaging_file_discard_mode() -> Result<PackageEnforcementMode, BusinessError> {
    let raw = std::env::var("ERPX_APPS_PACKAGE_ENFORCEMENT")
        .unwrap_or_else(|_| "disabled".to_owned());
    let mode = match raw.as_str() {
        "disabled" => PackageEnforcementMode::Disabled,
        "shadow" => PackageEnforcementMode::Shadow,
        "enforced" => PackageEnforcementMode::Enforced,
        _ => {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "PACKAGE_ENFORCEMENT_CONFIGURATION",
                "Package enforcement is unavailable.",
            ));
        }
    };
    if mode != PackageEnforcementMode::Disabled
        && std::env::var("ERPX_ENVIRONMENT").as_deref() != Ok("local")
    {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "PACKAGE_ENFORCEMENT_ENVIRONMENT",
            "Package enforcement is unavailable in this environment.",
        ));
    }
    Ok(mode)
}

/// Settle the existing Messages purchase obligation for an unsent file
/// discard. The boundary attaches to the Messaging serializable unit before the
/// owner write and re-evaluates `work.messages`/`settle-existing` afterwards.
async fn begin_messaging_file_discard_purchase(
    connection: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    key: &str,
) -> Result<Option<(PurchaseCommandBoundary, CommercialCatalogue)>, Error> {
    let mode = messaging_file_discard_mode().map_err(Error::Business)?;
    if mode == PackageEnforcementMode::Disabled {
        return Ok(None);
    }
    let catalogue = current_package_catalogue().map_err(Error::Business)?;
    let purchase_key = format!("{key}:file-discard");
    let Some(mut boundary) = PurchaseCommandBoundary::new(
        mode,
        purchase_key,
        PurchaseCommandScope {
            company_id: company.clone(),
            capability: "work.messages".into(),
            purpose: CapabilityPurpose::SettleExisting,
        },
    )
    .map_err(Error::Business)?
    else {
        return Ok(None);
    };
    boundary
        .begin(connection, holding, &catalogue)
        .await?;
    Ok(Some((boundary, catalogue)))
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

fn collaboration_invalid_source() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "SOURCE_RECORD_NOT_FOUND",
        "Source record is unavailable in this work scope.",
    )
}

fn source_href(kind: erpx_server_shared::public::record_source_contract::SourceKind, id: &str) -> Option<String> {
    use erpx_server_shared::public::record_source_contract::SourceKind;
    let encoded = erpx_server_collaboration::public::inbox::encode_uri_component(id);
    let prefix = match kind {
        SourceKind::CommerceOrder => "/commerce/orders/",
        SourceKind::SupportTicket => "/support/",
        SourceKind::FieldServiceOrder => "/field-service/",
        SourceKind::PosSale => "/pos?sale=",
        SourceKind::Asset => "/assets/",
        SourceKind::MaintenancePlan => "/maintenance/plans/",
        SourceKind::MaintenanceWork => "/maintenance/",
        SourceKind::StaffTask => "/staff-tasks/",
        SourceKind::PmsRecord => return None,
    };
    Some(format!("{prefix}{encoded}"))
}

enum CollaborationSourceWitness {
    Pos(erpx_server_pos::public::record_source::CurrentRecordSource),
    Pms(erpx_server_pms::public::record_source::CurrentRecordSource),
    Assets(erpx_server_assets::public::record_source::CurrentRecordSource),
    Commerce(erpx_server_commerce::public::record_source::CurrentRecordSource),
    Support(erpx_server_support::public::record_source::CurrentRecordSource),
    Maintenance(erpx_server_maintenance::public::record_source::CurrentRecordSource),
    Operations(erpx_server_operations::public::record_source::CurrentRecordSource),
}

/// Composition-owned source resolver over the retained serializable unit.
struct CollaborationSourceResolver {
    company: CompanyId,
}

impl erpx_server_collaboration::public::collaboration_work::CollaborationSourcePort
    for CollaborationSourceResolver
{
    type Error = Error;
    type Witness = CollaborationSourceWitness;

    fn prepare(
        &mut self,
        _source: &erpx_server_shared::public::record_source_contract::SourceReference,
    ) -> impl std::future::Future<
        Output = Result<
            (
                erpx_server_shared::public::record_source_contract::SourceRecord,
                Self::Witness,
            ),
            Self::Error,
        >,
    > + Send {
        Box::pin(async { Err(collaboration_invalid_source().into()) })
    }

    fn prepare_with<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        source: &'a erpx_server_shared::public::record_source_contract::SourceReference,
    ) -> impl std::future::Future<
        Output = Result<
            (
                erpx_server_shared::public::record_source_contract::SourceRecord,
                Self::Witness,
            ),
            Self::Error,
        >,
    > + Send {
        use erpx_server_shared::public::record_source_contract::SourceKind;
        Box::pin(async move {
            let company = self.company.clone();
            let _ = connection;
            match source.kind {
                SourceKind::PosSale => {
                    let current = erpx_server_pos::public::record_source::read_record_source(
                        connection,
                        &company,
                        &source.id,
                    )
                    .await?
                    .ok_or_else(collaboration_invalid_source)?;
                    let record = current.data().clone();
                    Ok((record, CollaborationSourceWitness::Pos(current)))
                }
                SourceKind::PmsRecord => {
                    let current = erpx_server_pms::public::record_source::read_record_source(
                        connection,
                        &company,
                        &source.id,
                    )
                    .await?
                    .ok_or_else(collaboration_invalid_source)?;
                    let record = current.data().clone();
                    Ok((record, CollaborationSourceWitness::Pms(current)))
                }
                SourceKind::Asset => {
                    let current = erpx_server_assets::public::record_source::read_record_source(
                        connection,
                        &company,
                        &source.id,
                    )
                    .await?
                    .ok_or_else(collaboration_invalid_source)?;
                    let record = current.data().clone();
                    Ok((record, CollaborationSourceWitness::Assets(current)))
                }
                SourceKind::CommerceOrder => {
                    let current = erpx_server_commerce::public::record_source::read_record_source(
                        connection,
                        &company,
                        &source.id,
                    )
                    .await?
                    .ok_or_else(collaboration_invalid_source)?;
                    let record = current.data().clone();
                    Ok((record, CollaborationSourceWitness::Commerce(current)))
                }
                SourceKind::SupportTicket => {
                    let current = erpx_server_support::public::record_source::read_record_source(
                        connection,
                        &company,
                        &source.id,
                    )
                    .await?
                    .ok_or_else(collaboration_invalid_source)?;
                    let record = current.data().clone();
                    Ok((record, CollaborationSourceWitness::Support(current)))
                }
                SourceKind::MaintenancePlan | SourceKind::MaintenanceWork => {
                    let current = erpx_server_maintenance::public::record_source::read_record_source(
                        connection,
                        &company,
                        source.kind,
                        &source.id,
                    )
                    .await?
                    .ok_or_else(collaboration_invalid_source)?;
                    let record = current.data().clone();
                    Ok((record, CollaborationSourceWitness::Maintenance(current)))
                }
                SourceKind::StaffTask => {
                    let current = erpx_server_operations::public::record_source::read_record_source(
                        connection,
                        &company,
                        &source.id,
                    )
                    .await?
                    .ok_or_else(collaboration_invalid_source)?;
                    let record = current.data().clone();
                    Ok((record, CollaborationSourceWitness::Operations(current)))
                }
                SourceKind::FieldServiceOrder => Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "NATIVE_FIELD_INBOX_REQUIRED",
                    "Field sources use the native Field workspace.",
                )
                .into()),
            }
        })
    }

    fn require_current(
        &mut self,
        _witness: &Self::Witness,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send {
        Box::pin(async { Ok(()) })
    }

    fn require_current_with<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        witness: &'a Self::Witness,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send {
        Box::pin(async move {
            match witness {
                CollaborationSourceWitness::Pos(current) => {
                    current.require_current(connection).await?
                }
                CollaborationSourceWitness::Pms(current) => {
                    current.require_current(connection).await?
                }
                CollaborationSourceWitness::Assets(current) => {
                    current.require_current(connection).await?
                }
                CollaborationSourceWitness::Commerce(current) => {
                    current.require_current(connection).await?
                }
                CollaborationSourceWitness::Support(current) => {
                    current.require_current(connection).await?
                }
                CollaborationSourceWitness::Maintenance(current) => {
                    current.require_current(connection).await?
                }
                CollaborationSourceWitness::Operations(current) => {
                    current.require_current(connection).await?
                }
            }
            Ok(())
        })
    }

    fn present(
        &mut self,
        source: &erpx_server_shared::public::record_source_contract::SourceReference,
    ) -> impl std::future::Future<
        Output = Result<
            erpx_server_collaboration::public::collaboration_work::SourcePresentation,
            Self::Error,
        >,
    > + Send {
        use erpx_server_collaboration::public::collaboration_work::SourcePresentation;
        Box::pin(async move {
            Ok(SourcePresentation {
                title: None,
                href: source_href(source.kind, &source.id),
            })
        })
    }
}

pub struct CollaborationWorkService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: SocialConfiguration,
    clock: Arc<C>,
}

fn collaboration_scope(
    current: &AuthorizedWorkContext,
    company: &CompanyId,
    now: i64,
) -> Result<
    erpx_server_collaboration::public::collaboration_work::CollaborationScope,
    BusinessError,
> {
    let selected = current.selection();
    let workspace = selected.workspace();
    Ok(
        erpx_server_collaboration::public::collaboration_work::CollaborationScope::new(
            company.as_str(),
            current.user_id().as_str(),
            selected.assignment().id.as_str(),
            now,
        )?
        .assignment_version(selected.assignment().version)
        .branch(workspace.branch_id.as_ref().map(|id| id.as_str()))
        .department(workspace.department_id.as_ref().map(|id| id.as_str())),
    )
}

impl<C: Clock + Send + Sync + 'static> CollaborationWorkService<C> {
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
        kind: erpx_server_collaboration::public::collaboration_work_http::CollaborationWorkReadKind,
    ) -> Result<
        erpx_server_collaboration::public::collaboration_work_http::CollaborationWorkHttpReadReply,
        Error,
    > {
        use erpx_server_collaboration::public::collaboration_work::{
            read_scope_card, read_scope_relation, read_scope_workspace,
        };
        use erpx_server_collaboration::public::collaboration_work_http::{
            CollaborationCardData, CollaborationRelationData, CollaborationWorkHttpReadReply,
            CollaborationWorkReadKind, CollaborationWorkspaceData,
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
                    destination: "collaboration".into(),
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
                        let scope = collaboration_scope(current, &company, now)?;
                        let mut sources = CollaborationSourceResolver {
                            company: company.clone(),
                        };
                        let can_write = current.has_permission("records.write");
                        let can_review = current.has_permission("records.review");
                        Ok(match kind {
                            CollaborationWorkReadKind::Workspace => {
                                let workspace =
                                    read_scope_workspace(connection, scope, &mut sources).await?;
                                CollaborationWorkHttpReadReply::Workspace(Box::new(
                                    CollaborationWorkspaceData {
                                        access: serde_json::json!({
                                            "canWrite": can_write,
                                            "canReview": can_review,
                                        }),
                                        relations: workspace.relations,
                                        cards: workspace.cards,
                                        people: workspace.people,
                                        source_types: workspace
                                            .source_types
                                            .into_iter()
                                            .map(str::to_owned)
                                            .collect(),
                                    },
                                ))
                            }
                            CollaborationWorkReadKind::Card { id } => {
                                let card =
                                    read_scope_card(connection, scope, &mut sources, &id).await?;
                                CollaborationWorkHttpReadReply::Card(Box::new(
                                    CollaborationCardData {
                                        access: serde_json::json!({
                                            "canWrite": can_write,
                                            "canReview": can_review,
                                        }),
                                        card: serde_json::json!({
                                            "id": card.id,
                                            "version": card.version,
                                            "open": card.open,
                                            "status": if card.open { "open" } else { "closed" },
                                            "owner_id": card.owner_id,
                                            "due_at": card.due_at,
                                            "owner_eligible": if card.owner_eligible { 1 } else { 0 },
                                            "source_type": card.source.kind.as_str(),
                                            "source_id": card.source.id,
                                        }),
                                    },
                                ))
                            }
                            CollaborationWorkReadKind::Relation { id } => {
                                let relation =
                                    read_scope_relation(connection, scope, &mut sources, &id)
                                        .await?;
                                CollaborationWorkHttpReadReply::Relation(Box::new(
                                    CollaborationRelationData {
                                        access: serde_json::json!({
                                            "canWrite": can_write,
                                            "canReview": can_review,
                                        }),
                                        relation: serde_json::json!({
                                            "id": relation.id,
                                            "version": relation.version,
                                            "status": if relation.active { "active" } else { "removed" },
                                            "source_type": relation.left.kind.as_str(),
                                            "source_id": relation.left.id,
                                            "target_type": relation.right.kind.as_str(),
                                            "target_id": relation.right.id,
                                        }),
                                    },
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
        change: erpx_server_collaboration::public::collaboration_work::Change,
        key: erpx_server_shared::public::CommandKey,
    ) -> Result<erpx_server_collaboration::public::collaboration_work::Receipt, Error> {
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
                    destination: "collaboration".into(),
                    api: true,
                },
                CollaborationWorkSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let change = change.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_error().into());
                        }
                        let now = clock.now().get();
                        let scope = collaboration_scope(current, &company, now)?;
                        let mut sources = CollaborationSourceResolver {
                            company: company.clone(),
                        };
                        erpx_server_collaboration::public::collaboration_work::execute_scope(
                            connection,
                            scope,
                            key,
                            &mut sources,
                            change,
                        )
                        .await
                    })
                },
            )
            .await
    }
}

pub struct CollaborationWorkBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<CollaborationWorkService<C>>,
}

impl<C: Clock + Send + Sync + 'static> CollaborationWorkBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<CollaborationWorkService<C>>,
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
    erpx_server_collaboration::public::collaboration_work_http::CollaborationWorkHttpBackend
    for CollaborationWorkBackend<C>
{
    fn read<'a>(
        &'a self,
        request: erpx_server_collaboration::public::collaboration_work_http::CollaborationWorkHttpReadRequest<'a>,
    ) -> erpx_server_collaboration::public::collaboration_work_http::CollaborationWorkHttpFuture<
        'a,
        erpx_server_collaboration::public::collaboration_work_http::CollaborationWorkHttpReadReply,
    > {
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
        request: erpx_server_collaboration::public::collaboration_work_http::CollaborationWorkHttpCommandRequest<'a>,
    ) -> erpx_server_collaboration::public::collaboration_work_http::CollaborationWorkHttpFuture<
        'a,
        erpx_server_collaboration::public::collaboration_work::Receipt,
    > {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .command(request.host, proof, selection, request.change, request.key)
                .await
                .map_err(business)
        })
    }
}
