//! Native Holding source administration, without HTTP activation. Normal
//! Account/Work/MFA verification precedes preview, decision and receipt replay.
use erpx_server_identity_access::public::{
    DeviceProof,
    account_security::AccountMfaKeyring,
    directory::{
        DirectorySourceAdministrationAction as Action, DirectorySourceChangeRequest,
        DirectorySourceReviewResult, DirectorySourceReviewStore, PostgresDirectory,
    },
    effective_authority::HoldingWorkAuthorizer,
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind, RecordId};
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;
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
}
pub struct DirectorySourceAdministration<C> {
    directory: Arc<PostgresDirectory>,
    authorizer: Arc<HoldingWorkAuthorizer>,
    keys: Arc<AccountMfaKeyring>,
    clock: Arc<C>,
}
enum ApplyState {
    Replay(DirectorySourceReviewResult),
    Pending(DirectorySourceChangeRequest),
}
fn error() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "DIRECTORY_SOURCE_REVIEW_CHANGED",
        "Directory source approval changed. Review the current proposal.",
    )
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
    pub async fn preview(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: HoldingWorkspaceRequest,
        input: DirectorySourcePreview,
    ) -> Result<DirectorySourceReviewResult, Error> {
        let request = input.request.clone();
        let key = input.key.clone();
        let reason = input.reason.clone();
        let clock = self.clock.clone();
        let replay = self
            .directory
            .execute_source_administration(
                &self.authorizer,
                host,
                proof.clone(),
                workspace.clone(),
                request.connection_id().clone(),
                Action::Propose,
                self.keys.clone(),
                move |c, current| {
                    let request = request.clone();
                    let key = key.clone();
                    let reason = reason.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        DirectorySourceReviewStore::replay_proposal(
                            c,
                            current,
                            &request,
                            &key,
                            &reason,
                            clock.now().get(),
                        )
                        .await
                    })
                },
            )
            .await?;
        if let Some(result) = replay {
            return Ok(result);
        }
        // All Control/Holding leases ended. Retaining the private profile is
        // local I/O and never runs inside a policy transaction.
        let change = self.directory.prepare_source_change(input.request).await?;
        let directory = self.directory.clone();
        let clock = self.clock.clone();
        self.directory
            .execute_source_administration(
                &self.authorizer,
                host,
                proof,
                workspace,
                RecordId::new(change.connection_id())?,
                Action::Propose,
                self.keys.clone(),
                move |c, current| {
                    let directory = directory.clone();
                    let change = change.clone();
                    let key = input.key.clone();
                    let reason = input.reason.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        directory.require_source_candidate(c, &change).await?;
                        DirectorySourceReviewStore::propose(
                            c,
                            current,
                            &change,
                            &key,
                            &reason,
                            clock.now().get(),
                        )
                        .await
                    })
                },
            )
            .await
    }
    /// Approval and application are one actual Holding command, matching the
    /// original independent-review decision boundary. There is no detached
    /// approval token that can outlive the current session/MFA/source checks.
    pub async fn approve_and_apply(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: HoldingWorkspaceRequest,
        input: DirectorySourceDecision,
    ) -> Result<DirectorySourceReviewResult, Error> {
        let decision = input.clone();
        let clock = self.clock.clone();
        let pending = self
            .directory
            .execute_source_administration(
                &self.authorizer,
                host,
                proof.clone(),
                workspace.clone(),
                input.connection_id.clone(),
                Action::Decide,
                self.keys.clone(),
                move |c, current| {
                    let input = decision.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if let Some(result) = DirectorySourceReviewStore::replay(
                            c,
                            current,
                            &input.review_id,
                            input.expected_version,
                            &input.key,
                            clock.now().get(),
                        )
                        .await?
                        {
                            return Ok(ApplyState::Replay(result));
                        }
                        let review = DirectorySourceReviewStore::read(
                            c,
                            current,
                            &input.review_id,
                            clock.now().get(),
                        )
                        .await?;
                        review.require_pending(input.expected_version, clock.now().get())?;
                        if review.connection_id()? != input.connection_id {
                            return Err(error().into());
                        }
                        Ok(ApplyState::Pending(review.source_request()?))
                    })
                },
            )
            .await?;
        let request = match pending {
            ApplyState::Replay(result) => return Ok(result),
            ApplyState::Pending(request) => request,
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
                    let input = input.clone();
                    let directory = directory.clone();
                    let clock = clock.clone();
                    let change = change.clone();
                    Box::pin(async move {
                        if let Some(result) = DirectorySourceReviewStore::replay(
                            c,
                            current,
                            &input.review_id,
                            input.expected_version,
                            &input.key,
                            clock.now().get(),
                        )
                        .await?
                        {
                            return Ok(result);
                        }
                        let review = DirectorySourceReviewStore::retain_apply(
                            c,
                            current,
                            &change,
                            &input.review_id,
                            input.expected_version,
                            &input.key,
                            clock.now().get(),
                        )
                        .await?;
                        directory.apply_reviewed_source(c, &change, &review).await?;
                        Ok(review.result())
                    })
                },
            )
            .await
    }
}
