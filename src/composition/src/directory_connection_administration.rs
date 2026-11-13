//! Native Directory connection transport backend. Account session, selected
//! Work authority, fresh MFA, provider I/O boundary and the owner transaction
//! stay in composition; the owner adapter owns receipts, audit and effects.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    account_security::AccountMfaKeyring,
    directory::PostgresDirectory,
    directory_connection::{
        DirectoryConnectionHttpBackend, DirectoryConnectionHttpCommand,
        DirectoryConnectionHttpFuture, DirectoryConnectionHttpRequest,
        DirectoryConnectionHttpResponse, DirectoryConnectionInput, PgDirectoryConnection,
        create_route, mode_supported, preview_json,
    },
    effective_authority::{HoldingWorkAuthorizer, WorkPermissionRequirement},
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_identity_access::public::directory::DirectoryProviderRead;
use erpx_server_runtime::public::postgres::{DatabasePool, PgRequestLease, PgTransactionError};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use serde_json::Value;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "DIRECTORY_CONNECTION_UNAVAILABLE",
        "The Directory connection service is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "DIRECTORY_CONNECTION_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

pub struct DirectoryConnectionBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    directory: Arc<PostgresDirectory>,
    authorizer: Arc<HoldingWorkAuthorizer>,
    provider: Arc<dyn DirectoryProviderRead>,
    keys: Arc<AccountMfaKeyring>,
    clock: Arc<C>,
    control: DatabasePool,
    holding: HoldingId,
    company: CompanyId,
}

impl<C: Clock + Send + Sync + 'static> DirectoryConnectionBackend<C> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        directory: Arc<PostgresDirectory>,
        authorizer: Arc<HoldingWorkAuthorizer>,
        provider: Arc<dyn DirectoryProviderRead>,
        keys: Arc<AccountMfaKeyring>,
        clock: Arc<C>,
        control: DatabasePool,
        holding: HoldingId,
        company: CompanyId,
    ) -> Self {
        Self {
            policy,
            scopes,
            directory,
            authorizer,
            provider,
            keys,
            clock,
            control,
            holding,
            company,
        }
    }

    fn context(
        &self,
        headers: &erpx_server_runtime::public::http_support::HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ));
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.company.clone(),
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: false,
            },
        ))
    }

    async fn save(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: HoldingWorkspaceRequest,
        input: Value,
        key: String,
    ) -> Result<Value, BusinessError> {
        let input = serde_json::from_value::<DirectoryConnectionInput>(input)
            .map_err(|_| {
                BusinessError::invalid(
                    "DIRECTORY_CONNECTION_REQUEST_INVALID",
                    "Review the connection request and try again.",
                )
            })?
            .validate()?;
        if !mode_supported(input.provider, input.config.mode) {
            return Err(BusinessError::invalid(
                "PROVIDER_MODE_INVALID",
                "Select a supported authentication method.",
            ));
        }
        let connection = input.id.clone();
        let allow_missing = true;
        let now = self.clock.now().get();
        let directory = self.directory.clone();
        let held = input.clone();
        let result = self
            .directory
            .execute_connection_administration_deferred(
                &self.authorizer,
                host,
                proof,
                move || Ok(workspace),
                connection,
                allow_missing,
                false,
                self.keys.clone(),
                move |c, current| {
                    let directory = directory.clone();
                    let input = held.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        current
                            .require_permission("iam.directory.manage")
                            .map_err(PgTransactionError::Business)?;
                        let mut tx = PgDirectoryConnection::new(c, &directory);
                        tx.save(current, input, &key, now).await
                    })
                },
            )
            .await
            .map_err(business)?;
        let id = result["id"].as_str().ok_or_else(unavailable)?;
        let control = self.control.clone();
        let holding = self.holding.as_str().to_owned();
        let company = self.company.as_str().to_owned();
        let id = id.to_owned();
        PgRequestLease::new(control)
            .run(move |c| {
                Box::pin(async move {
                    let result: Result<(), PgTransactionError<BusinessError>> = async {
                        create_route(c, &id, &holding, &company).await?;
                        Ok(())
                    }
                    .await;
                    result
                })
            })
            .await
            .map_err(|_| unavailable())?;
        Ok(result)
    }

    async fn test(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: HoldingWorkspaceRequest,
        id: String,
        version: i64,
        key: String,
    ) -> Result<Value, BusinessError> {
        let now = self.clock.now().get();
        let directory = self.directory.clone();
        let authorize_key = format!("{key}:authorize");
        let (descriptor, _, _) = self
            .directory
            .execute_connection_administration_deferred(
                &self.authorizer,
                host,
                proof.clone(),
                {
                    let workspace = workspace.clone();
                    move || Ok(workspace)
                },
                Some(id.clone()),
                false,
                false,
                self.keys.clone(),
                {
                    let directory = directory.clone();
                    let authorize_key = authorize_key.clone();
                    let id = id.clone();
                    move |c, current| {
                        let directory = directory.clone();
                        let authorize_key = authorize_key.clone();
                        let id = id.clone();
                        Box::pin(async move {
                            current
                                .require_permission("iam.directory.manage")
                                .map_err(PgTransactionError::Business)?;
                            let mut tx = PgDirectoryConnection::new(c, &directory);
                            tx.authorize_test(current, &id, version, &authorize_key, now)
                                .await
                        })
                    }
                },
            )
            .await
            .map_err(business)?;
        let snapshot = match self.provider.observe(&descriptor, now).await {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let directory = self.directory.clone();
                let id = id.clone();
                let code = error.code;
                let _ = self
                    .directory
                    .execute_connection_administration_deferred(
                        &self.authorizer,
                        host,
                        proof,
                        move || Ok(workspace),
                        Some(id.clone()),
                        false,
                        false,
                        self.keys.clone(),
                        move |c, current| {
                            let directory = directory.clone();
                            let id = id.clone();
                            let code = code.clone();
                            Box::pin(async move {
                                let mut tx = PgDirectoryConnection::new(c, &directory);
                                tx.record_test_failure(current, &id, version, code)
                                    .await
                            })
                        },
                    )
                    .await;
                return Err(error);
            }
        };
        let preview = preview_json(&snapshot);
        let directory = self.directory.clone();
        self.directory
            .execute_connection_administration_deferred(
                &self.authorizer,
                host,
                proof,
                move || Ok(workspace),
                Some(id.clone()),
                false,
                false,
                self.keys.clone(),
                move |c, current| {
                    let directory = directory.clone();
                    let id = id.clone();
                    let key = key.clone();
                    let preview = preview.clone();
                    Box::pin(async move {
                        current
                            .require_permission("iam.directory.manage")
                            .map_err(PgTransactionError::Business)?;
                        let mut tx = PgDirectoryConnection::new(c, &directory);
                        tx.record_tested(current, &id, version, preview, &key, now)
                            .await
                    })
                },
            )
            .await
            .map_err(business)
    }

    async fn activate(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: HoldingWorkspaceRequest,
        id: String,
        version: i64,
        key: String,
    ) -> Result<Value, BusinessError> {
        let now = self.clock.now().get();
        let directory = self.directory.clone();
        self.directory
            .execute_connection_administration_deferred(
                &self.authorizer,
                host,
                proof,
                move || Ok(workspace),
                Some(id.clone()),
                false,
                true,
                self.keys.clone(),
                move |c, current| {
                    let directory = directory.clone();
                    let id = id.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        current
                            .require_permission("iam.directory.manage")
                            .map_err(PgTransactionError::Business)?;
                        let mut tx = PgDirectoryConnection::new(c, &directory);
                        tx.activate(current, &id, version, &key, now).await
                    })
                },
            )
            .await
            .map_err(business)
    }

    async fn disable(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: HoldingWorkspaceRequest,
        id: String,
        version: i64,
        key: String,
    ) -> Result<Value, BusinessError> {
        let now = self.clock.now().get();
        let directory = self.directory.clone();
        self.directory
            .execute_connection_administration_deferred(
                &self.authorizer,
                host,
                proof,
                move || Ok(workspace),
                Some(id.clone()),
                false,
                true,
                self.keys.clone(),
                move |c, current| {
                    let directory = directory.clone();
                    let id = id.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        current
                            .require_permission("iam.directory.manage")
                            .map_err(PgTransactionError::Business)?;
                        let mut tx = PgDirectoryConnection::new(c, &directory);
                        tx.disable(current, &id, version, &key, now).await
                    })
                },
            )
            .await
            .map_err(business)
    }
}

impl<C: Clock + Send + Sync + 'static> DirectoryConnectionHttpBackend
    for DirectoryConnectionBackend<C>
{
    fn execute<'a>(
        &'a self,
        request: DirectoryConnectionHttpRequest<'a>,
    ) -> DirectoryConnectionHttpFuture<'a, DirectoryConnectionHttpResponse> {
        Box::pin(async move {
            let (proof, workspace) = self.context(request.headers)?;
            if workspace.company_id != self.company {
                return Err(scope());
            }
            let data = match request.command {
                DirectoryConnectionHttpCommand::Save {
                    input,
                    idempotency_key,
                } => {
                    self.save(request.host, proof, workspace, input, idempotency_key)
                        .await?
                }
                DirectoryConnectionHttpCommand::Test {
                    id,
                    version,
                    idempotency_key,
                } => {
                    self.test(request.host, proof, workspace, id, version, idempotency_key)
                        .await?
                }
                DirectoryConnectionHttpCommand::Activate {
                    id,
                    version,
                    idempotency_key,
                } => {
                    self.activate(request.host, proof, workspace, id, version, idempotency_key)
                        .await?
                }
                DirectoryConnectionHttpCommand::Disable {
                    id,
                    version,
                    idempotency_key,
                } => {
                    self.disable(request.host, proof, workspace, id, version, idempotency_key)
                        .await?
                }
            };
            Ok(DirectoryConnectionHttpResponse { data })
        })
    }
}

#[allow(dead_code)]
fn _permission() -> WorkPermissionRequirement {
    WorkPermissionRequirement::Exact("iam.directory.manage".into())
}
