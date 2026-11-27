//! Records search on the Holding transaction which validated the current Work selection.
use erpx_server_identity_access::public::{
    AccountPolicy, RequestAudience,
    account_inbound_http::{
        CapturedHoldingMutation, parse_inbound_account_request, unique_request_header,
    },
    account_work_presentation::{
        lock_account_work_presentation_sources, read_account_work_presentation,
    },
    effective_authority::HoldingWorkSourcePreparation,
    records_principal_search::{active_assignment_on, search_principals_on},
    work_context::HoldingWorkContextInspector,
};
use erpx_server_records::public::{
    record_search::{
        PrincipalHit, RecordHit, RecordSearchRead, SearchActor, SearchFuture, SearchQuery,
        SearchResponse, execute_search, read_search_candidates, request_invalid, unavailable,
        valid_slug,
    },
    record_search_http::RecordSearchBackend,
};
use erpx_server_runtime::public::{
    http_support::HeaderMap,
    postgres::{PgTransactionError, PgWorkFuture},
};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use sqlx::PgConnection;
use std::sync::Arc;

#[derive(Clone)]
struct SearchSources;

impl HoldingWorkSourcePreparation for SearchSources {
    fn prepare<'c>(
        &'c self,
        connection: &'c mut PgConnection,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(lock_account_work_presentation_sources(connection))
    }
}

struct SearchRead<'c> {
    holding: &'c mut PgConnection,
    control: &'c mut PgConnection,
    field_probe: &'static str,
}

impl RecordSearchRead for SearchRead<'_> {
    fn records<'a>(
        &'a mut self,
        actor: &'a SearchActor,
        query: &'a SearchQuery,
    ) -> SearchFuture<'a, Vec<RecordHit>> {
        Box::pin(read_search_candidates(
            self.holding,
            actor,
            query,
            self.field_probe,
        ))
    }

    fn principal_hits<'a>(
        &'a mut self,
        needle: &'a str,
        now: i64,
    ) -> SearchFuture<'a, Vec<PrincipalHit>> {
        Box::pin(async move {
            let mut hits = Vec::new();
            for (target_id, label) in search_principals_on(self.control, needle).await? {
                if active_assignment_on(self.holding, &target_id, now).await? {
                    hits.push(PrincipalHit { target_id, label });
                }
            }
            Ok(hits)
        })
    }
}

pub(crate) struct RecordsSearchBackend {
    policy: Arc<AccountPolicy>,
    inspector: HoldingWorkContextInspector,
    clock: Arc<dyn Clock + Send + Sync>,
    field_probe: &'static str,
}

impl RecordsSearchBackend {
    pub(crate) fn new(
        policy: Arc<AccountPolicy>,
        inspector: HoldingWorkContextInspector,
        clock: Arc<dyn Clock + Send + Sync>,
    ) -> Self {
        Self {
            policy,
            inspector,
            clock,
            field_probe: erpx_server_field_service::public::record_probe::field_record_probe(
                "sourceReadScopeProbe",
            ),
        }
    }
}

fn slug(headers: &HeaderMap, name: &'static str) -> Result<Option<String>, BusinessError> {
    unique_request_header(headers, name)?
        .map(|value| {
            valid_slug(value)
                .then(|| value.to_owned())
                .ok_or_else(request_invalid)
        })
        .transpose()
}

impl RecordSearchBackend for RecordsSearchBackend {
    fn search<'a>(
        &'a self,
        host: &'a str,
        headers: &'a HeaderMap,
        query: &'a SearchQuery,
    ) -> SearchFuture<'a, SearchResponse> {
        Box::pin(async move {
            let inbound = parse_inbound_account_request(&self.policy, headers)?;
            if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
                return Err(BusinessError::new(
                    FailureKind::NotFound,
                    "NOT_FOUND",
                    "Not found.",
                ));
            }
            let headers = headers.clone();
            let query = query.clone();
            let clock = self.clock.clone();
            let field_probe = self.field_probe;
            self.inspector
                .with_holding_account_and_control(
                    host,
                    inbound.into_proof(),
                    SearchSources,
                    move |connection, control, account| {
                        let headers = headers.clone();
                        let query = query.clone();
                        let clock = clock.clone();
                        Box::pin(async move {
                            let selection = CapturedHoldingMutation::capture(
                                &headers,
                                slug(&headers, "x-erpx-workspace-branch")?,
                                slug(&headers, "x-erpx-workspace-department")?,
                            )
                            .resolve_rendered_read()?;
                            let scope = account.workspace(selection);
                            let now = clock.now().get();
                            let presentation = read_account_work_presentation(
                                connection,
                                &scope,
                                "records/search",
                                true,
                                now,
                            )
                            .await?;
                            let current = &presentation.context;
                            let selected = current.selection();
                            let workspace = selected.workspace();
                            let actor = SearchActor {
                                user_id: current.user_id().as_str().to_owned(),
                                assignment_id: selected.assignment().id.as_str().to_owned(),
                                company_id: workspace.company_id.as_str().to_owned(),
                                branch_id: workspace
                                    .branch_id
                                    .as_ref()
                                    .map(|id| id.as_str().to_owned()),
                                department_id: workspace
                                    .department_id
                                    .as_ref()
                                    .map(|id| id.as_str().to_owned()),
                                modules: current.modules().to_vec(),
                            };
                            let mut read = SearchRead {
                                holding: connection,
                                control,
                                field_probe,
                            };
                            let result = execute_search(&mut read, &actor, &query, now).await?;
                            if clock.now().get() >= current.expires_at() {
                                return Err(BusinessError::new(
                                    FailureKind::Conflict,
                                    "WORK_CONTEXT_CHANGED",
                                    "Your work role changed. Reload and try again.",
                                )
                                .into());
                            }
                            scope.require_current(connection, clock.now().get()).await?;
                            Ok(result)
                        })
                    },
                )
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    PgTransactionError::Persistence(_) => unavailable(),
                })
        })
    }
}
