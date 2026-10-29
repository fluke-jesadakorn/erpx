//! Native Holding source administration. Only the existing coordinator verifies
//! sessions/MFA and mints current IAM; HTTP selectors and bodies stay deferred.
use erpx_server_identity_access::public::{
    DeviceProof,
    account_security::AccountMfaKeyring,
    directory::{
        DirectorySourceAdministrationAction as Action, DirectorySourceChangeRequest,
        DirectorySourceReviewResult, DirectorySourceReviewStore, PostgresDirectory,
        normalize_directory_source_reason,
    },
    effective_authority::{AuthorizedWorkContext, HoldingWorkAuthorizer},
    work_context::{ExpectedWorkSelection, HoldingWorkspaceRequest},
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind, RecordId};
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;
#[derive(Clone)]
pub struct DirectorySourcePreview {
    pub request: DirectorySourceChangeRequest,
    pub key: CommandKey,
    pub reason: String,
}
#[derive(Clone)]
pub struct DirectorySourceDecision {
    pub connection_id: RecordId,
    pub review_id: RecordId,
    pub expected_version: i64,
    pub key: CommandKey,
    pub reason: String,
}
pub struct DirectorySourceAdministration<C> {
    directory: Arc<PostgresDirectory>,
    authorizer: Arc<HoldingWorkAuthorizer>,
    keys: Arc<AccountMfaKeyring>,
    clock: Arc<C>,
}
enum PreviewState {
    Replay(DirectorySourceReviewResult),
    Pending(Box<(DirectorySourcePreview, HoldingWorkspaceRequest)>),
}
enum ApplyState {
    Replay(DirectorySourceReviewResult),
    Pending(
        Box<(
            DirectorySourceChangeRequest,
            DirectorySourceDecision,
            HoldingWorkspaceRequest,
        )>,
    ),
}
fn error() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "DIRECTORY_SOURCE_REVIEW_CHANGED",
        "Directory source approval changed. Review the current proposal.",
    )
}
/// Reuse the original verified selection as CAS INPUT after private I/O. This
/// data cannot create a session, permission or current authority context.
fn captured_workspace(current: &AuthorizedWorkContext) -> HoldingWorkspaceRequest {
    let s = current.selection();
    HoldingWorkspaceRequest {
        company_id: s.workspace().company_id.clone(),
        branch_slug: s.workspace().branch_slug.clone(),
        department_slug: s.workspace().department_slug.clone(),
        expected: Some(ExpectedWorkSelection {
            revision: s.revision(),
            assignment_id: s.assignment().id.clone(),
            assignment_version: s.assignment().version,
        }),
        mutation: true,
    }
}
impl<C: Clock + Send + Sync + 'static> DirectorySourceAdministration<C> {
    pub fn new(
        directory: Arc<PostgresDirectory>,
        authorizer: Arc<HoldingWorkAuthorizer>,
        keys: Arc<AccountMfaKeyring>,
        clock: Arc<C>,
    ) -> Self {
        Self {
            directory,
            authorizer,
            keys,
            clock,
        }
    }
    /// Internal resolved convenience; inbound HTTP must use preview_transport.
    pub async fn preview(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: HoldingWorkspaceRequest,
        input: DirectorySourcePreview,
    ) -> Result<DirectorySourceReviewResult, Error> {
        self.preview_transport(
            host,
            proof,
            move || Ok(workspace),
            input.request.connection_id().as_str().into(),
            move || Ok(input.clone()),
        )
        .await
    }
    /// Every selector is resolved once after live Control authentication, and
    /// body/key normalization runs after the same fresh-MFA and current IAM hook.
    pub async fn preview_transport<Input, Prepare>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        connection: String,
        prepare: Prepare,
    ) -> Result<DirectorySourceReviewResult, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        Prepare:
            Fn() -> Result<DirectorySourcePreview, BusinessError> + Clone + Send + Sync + 'static,
    {
        let target = connection.clone();
        let clock = self.clock.clone();
        let state = self
            .directory
            .execute_source_administration_deferred(
                &self.authorizer,
                host,
                proof.clone(),
                workspace,
                connection,
                Action::Propose,
                self.keys.clone(),
                move |c, current| {
                    let prepare = prepare.clone();
                    let target = target.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        let mut input = prepare()?;
                        input.reason = normalize_directory_source_reason(&input.reason)?;
                        if input.request.connection_id().as_str() != target {
                            return Err(error().into());
                        }
                        if let Some(result) = DirectorySourceReviewStore::replay_proposal(
                            c,
                            current,
                            &input.request,
                            &input.key,
                            &input.reason,
                            clock.now().get(),
                        )
                        .await?
                        {
                            return Ok(PreviewState::Replay(result));
                        }
                        Ok(PreviewState::Pending(Box::new((
                            input,
                            captured_workspace(current),
                        ))))
                    })
                },
            )
            .await?;
        let (input, workspace) = match state {
            PreviewState::Replay(result) => return Ok(result),
            PreviewState::Pending(prepared) => *prepared,
        };
        // No database lease is held during private profile retention.
        let change = self
            .directory
            .prepare_source_change(input.request.clone())
            .await?;
        let directory = self.directory.clone();
        let clock = self.clock.clone();
        self.directory
            .execute_source_administration(
                &self.authorizer,
                host,
                proof,
                workspace,
                input.request.connection_id().clone(),
                Action::Propose,
                self.keys.clone(),
                move |c, current| {
                    let directory = directory.clone();
                    let change = change.clone();
                    let input = input.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if let Some(result) = DirectorySourceReviewStore::replay_proposal(
                            c,
                            current,
                            &input.request,
                            &input.key,
                            &input.reason,
                            clock.now().get(),
                        )
                        .await?
                        {
                            return Ok(result);
                        }
                        directory.require_source_candidate(c, &change).await?;
                        DirectorySourceReviewStore::propose(
                            c,
                            current,
                            &change,
                            &input.key,
                            &input.reason,
                            clock.now().get(),
                        )
                        .await
                    })
                },
            )
            .await
    }
    /// Internal resolved convenience; approval and source effects still use the
    /// same current session/MFA, source CAS and owner transaction as HTTP.
    pub async fn approve_and_apply(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: HoldingWorkspaceRequest,
        input: DirectorySourceDecision,
    ) -> Result<DirectorySourceReviewResult, Error> {
        self.approve_transport(
            host,
            proof,
            move || Ok(workspace),
            input.connection_id.as_str().into(),
            input.review_id.as_str().into(),
            move || Ok(input.clone()),
        )
        .await
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn approve_transport<Input, Prepare>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        connection: String,
        review: String,
        prepare: Prepare,
    ) -> Result<DirectorySourceReviewResult, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        Prepare:
            Fn() -> Result<DirectorySourceDecision, BusinessError> + Clone + Send + Sync + 'static,
    {
        let target = connection.clone();
        let clock = self.clock.clone();
        let state = self
            .directory
            .execute_source_administration_deferred(
                &self.authorizer,
                host,
                proof.clone(),
                workspace,
                connection,
                Action::Decide,
                self.keys.clone(),
                move |c, current| {
                    let prepare = prepare.clone();
                    let target = target.clone();
                    let review = review.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        let mut input = prepare()?;
                        input.reason = normalize_directory_source_reason(&input.reason)?;
                        if input.connection_id.as_str() != target
                            || input.review_id.as_str() != review
                        {
                            return Err(error().into());
                        }
                        if let Some(result) = DirectorySourceReviewStore::replay(
                            c,
                            current,
                            &input.review_id,
                            input.expected_version,
                            &input.key,
                            &input.reason,
                            clock.now().get(),
                        )
                        .await?
                        {
                            return Ok(ApplyState::Replay(result));
                        }
                        let stored = DirectorySourceReviewStore::read(
                            c,
                            current,
                            &input.review_id,
                            clock.now().get(),
                        )
                        .await?;
                        stored.require_pending(input.expected_version, clock.now().get())?;
                        if stored.connection_id()? != input.connection_id {
                            return Err(error().into());
                        }
                        Ok(ApplyState::Pending(Box::new((
                            stored.source_request()?,
                            input,
                            captured_workspace(current),
                        ))))
                    })
                },
            )
            .await?;
        let (request, input, workspace) = match state {
            ApplyState::Replay(result) => return Ok(result),
            ApplyState::Pending(prepared) => *prepared,
        };
        let change = self.directory.prepare_source_change(request).await?;
        let directory = self.directory.clone();
        let clock = self.clock.clone();
        self.directory
            .execute_source_administration(
                &self.authorizer,
                host,
                proof,
                workspace,
                input.connection_id.clone(),
                Action::Decide,
                self.keys.clone(),
                move |c, current| {
                    let directory = directory.clone();
                    let change = change.clone();
                    let input = input.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if let Some(result) = DirectorySourceReviewStore::replay(
                            c,
                            current,
                            &input.review_id,
                            input.expected_version,
                            &input.key,
                            &input.reason,
                            clock.now().get(),
                        )
                        .await?
                        {
                            return Ok(result);
                        }
                        let approved = DirectorySourceReviewStore::retain_apply(
                            c,
                            current,
                            &change,
                            &input.review_id,
                            input.expected_version,
                            &input.key,
                            &input.reason,
                            clock.now().get(),
                        )
                        .await?;
                        directory
                            .apply_reviewed_source(c, &change, &approved)
                            .await?;
                        Ok(approved.result())
                    })
                },
            )
            .await
    }
}
