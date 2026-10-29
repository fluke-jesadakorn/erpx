//! Same original Control lease, exact-session MFA and route pins for source
//! administration. Only the existing Work coordinator mints its IAM context.
use super::{
    account_mfa_crypto::AccountMfaKeyring,
    account_mfa_postgres::{FreshMfaProof, require_fresh_mfa_in_transaction},
    authorized_work::{
        HoldingWorkAuthorizer, IdentityWorkSourcePreparation, RetainedWorkControlSources,
        RetainedWorkHoldingSources, WorkControlSources,
    },
    directory_postgres::{
        DirectoryRoute, PostgresDirectory, connection_without_source, matches_descriptor,
    },
    directory_source_review::DirectorySourceReviewPreparation,
};
use crate::{
    application::account_ports::DeviceProof,
    domain::{
        account_auth::AccountSession,
        directory::DirectoryConnectionDescriptor,
        directory_source_review::{DirectoryAdministrationProof, review_error},
        effective_authority::{AuthorizedWorkContext, WorkAuthorizationRequest},
        work_context::{HoldingWorkspaceRequest, WorkContextRequest},
    },
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, RecordId};
use sqlx::{PgConnection, Row};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub enum DirectorySourceAdministrationAction {
    Propose,
    Decide,
}
impl DirectorySourceAdministrationAction {
    pub(super) fn permission(self) -> &'static str {
        match self {
            Self::Propose => "iam.directory.manage",
            Self::Decide => "iam.review",
        }
    }
}
struct AdminSources {
    directory: Arc<PostgresDirectory>,
    connection: String,
    proof: DeviceProof,
    keys: Arc<AccountMfaKeyring>,
}
struct RetainedControl {
    mfa: FreshMfaProof,
    directory: Arc<PostgresDirectory>,
    route: Arc<DirectoryRoute>,
    connection: RecordId,
}
struct RetainedHolding {
    route: Arc<DirectoryRoute>,
    descriptor: DirectoryConnectionDescriptor,
    deadline: i64,
    proof: DirectoryAdministrationProof,
}
impl WorkControlSources for AdminSources {
    fn prepare_control<'c>(
        &'c self,
        c: &'c mut PgConnection,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            // The canonical coordinator already took session-first utility
            // fences. Neither statement below establishes its first snapshot.
            sqlx::query("LOCK TABLE iam_mfa,iam_mfa_sessions IN SHARE MODE")
                .execute(&mut *c)
                .await?;
            PostgresDirectory::lock_routes(c).await?;
            Ok(())
        })
    }
    fn retain_control<'c>(
        &'c self,
        c: &'c mut PgConnection,
        session: &'c AccountSession,
        input: &'c HoldingWorkspaceRequest,
        now: i64,
    ) -> PgWorkFuture<'c, Arc<dyn RetainedWorkControlSources>, BusinessError> {
        Box::pin(async move {
            let connection = RecordId::new(&self.connection)?;
            let route = self.directory.route(c, connection.as_str()).await?;
            let holding = route
                .holding_id
                .as_ref()
                .ok_or_else(|| review_error("DIRECTORY_SOURCE_PLATFORM_UNAVAILABLE"))?;
            if route.company_id != input.company_id.as_str()
                || input.expected.is_none()
                || !input.mutation
            {
                return Err(review_error("DIRECTORY_SOURCE_ADMIN_SCOPE_REQUIRED").into());
            }
            let audience = self.directory.registry.audience_for_holding(holding)?;
            let mfa = require_fresh_mfa_in_transaction(
                c,
                &self.proof,
                &audience,
                &self.directory.environment,
                self.keys.clone(),
                now,
                &self.directory.registry,
            )
            .await?;
            if mfa.session_id() != session.id || mfa.user_id() != session.user.id {
                return Err(review_error("DIRECTORY_SOURCE_ADMIN_SCOPE_REQUIRED").into());
            }
            Ok(Arc::new(RetainedControl {
                mfa,
                directory: self.directory.clone(),
                route: Arc::new(route),
                connection,
            }) as Arc<dyn RetainedWorkControlSources>)
        })
    }
}
impl RetainedWorkControlSources for RetainedControl {
    fn prepare_holding<'c>(
        &'c self,
        c: &'c mut PgConnection,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move { DirectorySourceReviewPreparation::locks(c).await })
    }
    fn retain_holding<'c>(
        &'c self,
        c: &'c mut PgConnection,
        request: &'c WorkContextRequest,
        now: i64,
    ) -> PgWorkFuture<'c, Box<dyn RetainedWorkHoldingSources>, BusinessError> {
        Box::pin(async move {
            if request.user_id.as_str() != self.mfa.user_id()
                || request.session_id.as_str() != self.mfa.session_id()
                || self.route.holding_id.as_ref() != Some(&request.holding_id)
                || self.route.company_id != request.company_id.as_str()
                || now >= self.mfa.expires_at()
            {
                return Err(review_error("DIRECTORY_SOURCE_ADMIN_SCOPE_REQUIRED").into());
            }
            let descriptor =
                connection_without_source(c, self.connection.as_str(), &self.route).await?;
            let row=sqlx::query("SELECT current_database() database,pg_backend_pid() backend,pg_current_xact_id()::text xid,pg_postmaster_start_time()::text postmaster").fetch_one(&mut*c).await?;
            let proof = DirectoryAdministrationProof {
                holding_id: request.holding_id.as_str().into(),
                company_id: request.company_id.as_str().into(),
                connection_id: self.connection.as_str().into(),
                user_id: self.mfa.user_id().into(),
                session_id: self.mfa.session_id().into(),
                deadline: self.mfa.expires_at(),
                unit: (
                    row.try_get("database")?,
                    row.try_get("backend")?,
                    row.try_get("xid")?,
                    row.try_get("postmaster")?,
                ),
            };
            Ok(Box::new(RetainedHolding {
                route: self.route.clone(),
                descriptor,
                deadline: self.mfa.expires_at(),
                proof,
            }) as Box<dyn RetainedWorkHoldingSources>)
        })
    }
    fn require_current<'c>(
        &'c self,
        c: &'c mut PgConnection,
        now: i64,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            self.mfa.require_current(c, now).await?;
            self.route.require_current(c).await?;
            // Keep the original configured binding; a local route selector is
            // not an alternate registry or a database fallback.
            if self.directory.environment != "development" {
                return Err(review_error("DIRECTORY_SOURCE_PLATFORM_UNAVAILABLE").into());
            }
            Ok(())
        })
    }
}
impl RetainedWorkHoldingSources for RetainedHolding {
    fn directory_administration(&self) -> Option<DirectoryAdministrationProof> {
        Some(self.proof.clone())
    }
    fn deadline(&self) -> i64 {
        self.deadline
    }
    fn require_current<'c>(
        &'c self,
        c: &'c mut PgConnection,
        now: i64,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            if now >= self.deadline {
                return Err(review_error("FRESH_MFA_REQUIRED").into());
            }
            matches_descriptor(
                &connection_without_source(c, &self.descriptor.connection_id, &self.route).await?,
                &self.descriptor,
            )?;
            Ok(())
        })
    }
}
impl PostgresDirectory {
    /// Additional source/MFA constraints only. Deferred selectors are consumed
    /// once by the canonical session-first coordinator. No provider I/O, second
    /// Control lease, Context constructor or caller-supplied authority is used.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute_source_administration<T: Send + 'static, F>(
        self: &Arc<Self>,
        authorizer: &HoldingWorkAuthorizer,
        host: &str,
        proof: DeviceProof,
        workspace: HoldingWorkspaceRequest,
        connection: RecordId,
        action: DirectorySourceAdministrationAction,
        keys: Arc<AccountMfaKeyring>,
        run: F,
    ) -> Result<T, PgTransactionError<BusinessError>>
    where
        F: for<'c> Fn(
                &'c mut PgConnection,
                &'c AuthorizedWorkContext,
            ) -> PgWorkFuture<'c, T, BusinessError>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        self.execute_source_administration_deferred(
            authorizer,
            host,
            proof,
            move || Ok(workspace),
            connection.as_str().into(),
            action,
            keys,
            run,
        )
        .await
    }
    /// Resolve HTTP selectors once, after live session verification. All MFA,
    /// route and current IAM evidence remains on the same retained path.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute_source_administration_deferred<T: Send + 'static, F, Input>(
        self: &Arc<Self>,
        authorizer: &HoldingWorkAuthorizer,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        connection: String,
        action: DirectorySourceAdministrationAction,
        keys: Arc<AccountMfaKeyring>,
        run: F,
    ) -> Result<T, PgTransactionError<BusinessError>>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        F: for<'c> Fn(
                &'c mut PgConnection,
                &'c AuthorizedWorkContext,
            ) -> PgWorkFuture<'c, T, BusinessError>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        let workspace = move || {
            let mut workspace = workspace()?;
            workspace.mutation = true;
            Ok(workspace)
        };
        let control = Arc::new(AdminSources {
            directory: self.clone(),
            connection,
            proof: proof.clone(),
            keys,
        });
        authorizer
            .execute_with_control_sources_deferred(
                host,
                proof,
                workspace,
                WorkAuthorizationRequest {
                    permission: action.permission().into(),
                    module: None,
                    destination: "iam/connections".into(),
                    api: true,
                },
                IdentityWorkSourcePreparation {
                    holding: DirectorySourceReviewPreparation,
                    control,
                },
                run,
            )
            .await
    }
}
