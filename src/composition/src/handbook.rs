//! Native Handbook read assembly. Current Account/Holding authority stays with
//! the selected coordinator; the owner supplies membership/editor facts and the
//! bounded article/list/editor rows.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_collaboration::public::handbook::{
    AuthorizedHandbookActor, ListCursor, read_article_detail, read_editor_options, read_list,
};
use erpx_server_collaboration::public::handbook_http::{
    HandbookDetailData, HandbookEditorsData, HandbookHttpBackend, HandbookHttpCommandRequest,
    HandbookHttpFuture, HandbookHttpReadReply, HandbookHttpReadRequest, HandbookListData,
    HandbookReadKind,
};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        HoldingWorkAuthorizer, HoldingWorkSourcePreparation, WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId};
use serde_json::json;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "HANDBOOK_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "HANDBOOK_UNAVAILABLE",
        "The handbook is unavailable.",
    )
}

fn command_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "HANDBOOK_COMMAND_UNAVAILABLE",
        "Native handbook commands are not enabled yet.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct HandbookSources;

impl HoldingWorkSourcePreparation for HandbookSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            sqlx::query(
                "LOCK TABLE handbook_articles,handbook_revisions,handbook_settings,handbook_editors IN SHARE ROW EXCLUSIVE MODE",
            )
            .execute(&mut *c)
            .await
            .map_err(|_| unavailable())?;
            Ok(())
        })
    }
}

#[derive(Clone)]
pub struct HandbookConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl HandbookConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

pub struct HandbookService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: HandbookConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> HandbookService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: HandbookConfiguration,
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

    fn access() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: "module.collaboration.read".into(),
            module: Some("collaboration".into()),
            destination: "handbook".into(),
            api: true,
        }
    }

    fn input<Input>(
        &self,
        input: Input,
    ) -> impl FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = false;
            Ok(request)
        }
    }

    pub async fn read<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        kind: HandbookReadKind,
    ) -> Result<HandbookHttpReadReply, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let _clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                HandbookSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let kind = kind.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("module.collaboration.read")?;
                        let user = current.user_id().as_str().to_owned();
                        let owner = current.selection().assignment().role == "holding_owner";
                        let editor = sqlx::query_scalar::<_, bool>(
                            "SELECT EXISTS(SELECT 1 FROM handbook_editors WHERE company_id=$1 AND user_id=$2)",
                        )
                        .bind(company.as_str())
                        .bind(&user)
                        .fetch_one(&mut *connection)
                        .await?;
                        let actor = AuthorizedHandbookActor {
                            user_id: user,
                            company_id: company.as_str().into(),
                            can_read: true,
                            can_edit: owner || editor,
                            can_manage_editors: owner,
                        };
                        let access = json!({"owner": owner, "editor": owner || editor});
                        let reply = match kind {
                            HandbookReadKind::List {
                                query,
                                editing,
                                cursor,
                                limit,
                            } => {
                                let cursor = cursor
                                    .as_deref()
                                    .map(ListCursor::decode)
                                    .transpose()
                                    .map_err(Error::from)?;
                                let page = read_list(
                                    connection,
                                    &actor,
                                    &company,
                                    &query,
                                    editing,
                                    cursor.as_ref(),
                                    limit,
                                )
                                .await?;
                                HandbookHttpReadReply::List(Box::new(
                                    HandbookListData::from_page(access, page),
                                ))
                            }
                            HandbookReadKind::Article {
                                id,
                                editing,
                                revision,
                            } => {
                                let read = read_article_detail(
                                    connection,
                                    &actor,
                                    &company,
                                    &id,
                                    editing,
                                    revision,
                                )
                                .await?;
                                HandbookHttpReadReply::Article(Box::new(
                                    HandbookDetailData::from_read(access, read),
                                ))
                            }
                            HandbookReadKind::Editors => {
                                let options =
                                    read_editor_options(connection, &actor, &company).await?;
                                HandbookHttpReadReply::Editors(Box::new(
                                    HandbookEditorsData::from(options),
                                ))
                            }
                        };
                        Ok(reply)
                    })
                },
            )
            .await
    }
}

pub struct HandbookBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<HandbookService<C>>,
}

impl<C: Clock + Send + Sync + 'static> HandbookBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<HandbookService<C>>,
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
                company_id: self.service.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: false,
            },
        ))
    }
}

impl<C: Clock + Send + Sync + 'static> HandbookHttpBackend for HandbookBackend<C> {
    fn read<'a>(
        &'a self,
        request: HandbookHttpReadRequest<'a>,
    ) -> HandbookHttpFuture<'a, HandbookHttpReadReply> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .read(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.kind,
                )
                .await
                .map_err(business)
        })
    }

    fn command<'a>(
        &'a self,
        _: HandbookHttpCommandRequest<'a>,
    ) -> HandbookHttpFuture<'a, erpx_server_collaboration::public::handbook::Receipt> {
        Box::pin(async move { Err(command_unavailable()) })
    }
}

// Retain the scope kind import for feature-specific builds.
#[allow(dead_code)]
fn _scope_kind(_: WorkScopeKind) {}
#[allow(dead_code)]
fn _key(_: CommandKey) {}
