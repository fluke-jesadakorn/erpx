//! Records search on the Holding transaction which validated the current Work selection.
use erpx_server_commercial::public::{
    capability_decision::PgCapabilityDecisionReader,
    purchases::{
        CapabilityPurpose, PackageEnforcementMode, current_package_catalogue,
        lock_commercial_authority_sources,
    },
};
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
        PrincipalHit, RecordHit, RecordSearchGuards, RecordSearchRead, SearchActor, SearchFuture,
        SearchQuery, SearchResponse, execute_search, read_search_candidates, request_invalid,
        unavailable, valid_slug,
    },
    record_search_http::RecordSearchBackend,
};
use erpx_server_runtime::public::{
    http_support::HeaderMap,
    postgres::{PgTransactionError, PgWorkFuture},
};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, TimestampMillis};
use sqlx::PgConnection;
use std::sync::Arc;

#[derive(Clone)]
struct SearchSources;

impl HoldingWorkSourcePreparation for SearchSources {
    fn prepare<'c>(
        &'c self,
        connection: &'c mut PgConnection,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async {
            lock_commercial_authority_sources(connection).await?;
            lock_account_work_presentation_sources(connection).await
        })
    }
}

struct SearchRead<'c> {
    holding: &'c mut PgConnection,
    control: &'c mut PgConnection,
    guards: RecordSearchGuards<'c>,
}

fn finance_enforcement_mode() -> Result<PackageEnforcementMode, BusinessError> {
    let raw =
        std::env::var("ERPX_FINANCE_PACKAGE_ENFORCEMENT").unwrap_or_else(|_| "disabled".to_owned());
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

async fn finance_accounting_history(
    connection: &mut PgConnection,
    company: &CompanyId,
    mode: PackageEnforcementMode,
    now: TimestampMillis,
) -> Result<bool, PgTransactionError<BusinessError>> {
    if mode == PackageEnforcementMode::Disabled {
        return Ok(true);
    }
    let catalogue = current_package_catalogue()?;
    let check = PgCapabilityDecisionReader::new(company.clone(), connection, &catalogue)
        .check("finance.accounting", CapabilityPurpose::History, mode, now)
        .await?;
    if mode == PackageEnforcementMode::Shadow {
        eprintln!(
            "{}",
            serde_json::json!({
                "event": "commercial.purchase.shadow",
                "phase": "read",
                "companyId": company.as_str(),
                "capability": "finance.accounting",
                "purpose": "history",
                "code": check.decision.as_ref().map(|decision| decision.code),
                "purchaseRevision": check.revision,
            }),
        );
    }
    Ok(check.allowed_by_mode)
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
            &self.guards,
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
                            let search_time = clock.now();
                            let now = search_time.get();
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
                            let finance_mode = finance_enforcement_mode()?;
                            let finance_predicate = if actor.modules.iter().any(|module| module == "finance") {
                                let accounting_history = finance_accounting_history(
                                    connection,
                                    &workspace.company_id,
                                    finance_mode,
                                    search_time,
                                ).await?;
                                erpx_server_finance::public::records_search_policy::records_search_finance_predicate(
                                    finance_mode == PackageEnforcementMode::Enforced,
                                    accounting_history,
                                    std::env::var("NODE_ENV").as_deref() == Ok("production"),
                                )
                            } else {
                                "TRUE".to_owned()
                            };
                            let mut read = SearchRead {
                                holding: connection,
                                control,
                                guards: RecordSearchGuards {
                                    field_probe,
                                    finance_predicate: &finance_predicate,
                                    assignment_role: &selected.assignment().role,
                                    now_ms: now,
                                },
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
