//! Native Work Projection report HTTP assembly.
//!
//! Composition supplies the current Account/Holding proof, one serializable
//! Holding unit and the already-validated report projection. The owner read
//! performs the matching, aggregation and pagination on those policy-scoped
//! rows; authority and presentation stay with their owners.
use crate::business_http::BusinessScopeSlugs;
use erpx_identity_access_server::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_platform_runtime_server::public::http_support::HeaderMap;
use erpx_platform_runtime_server::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind, HoldingId};
use erpx_work_projection_server::public::work_report_read::{
    WorkReportProjection, read_work_report_projection,
};
use erpx_work_projection_server::public::work_report_read_http::{
    WorkReportHttpBackend, WorkReportHttpFuture, WorkReportHttpProjection, WorkReportHttpRequest,
    WorkReportHttpResponse, native_work_report_json,
};
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "WORK_REPORT_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "WORK_REPORT_UNAVAILABLE",
        "The native Work report is unavailable.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct WorkReportSources;

impl HoldingWorkSourcePreparation for WorkReportSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

fn context(current: &AuthorizedWorkContext) -> serde_json::Value {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    serde_json::json!({
        "userId": current.user_id().as_str(),
        "holdingId": current.holding_id().as_str(),
        "companyId": workspace.company_id.as_str(),
        "assignmentId": assignment.id.as_str(),
        "revision": selected.revision(),
        "expiresAt": current.expires_at(),
    })
}

#[derive(Clone)]
pub struct NativeWorkReportConfiguration {
    holding: HoldingId,
}

impl NativeWorkReportConfiguration {
    pub fn new(holding: HoldingId) -> Self {
        Self { holding }
    }
}

pub struct NativeWorkReportService {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: NativeWorkReportConfiguration,
}

impl NativeWorkReportService {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: NativeWorkReportConfiguration,
    ) -> Self {
        Self {
            authorizer,
            configuration,
        }
    }

    fn access() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: "self.tasks".into(),
            module: None,
            destination: "work/native-report".into(),
            api: true,
        }
    }

    pub async fn read<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        report: &WorkReportHttpProjection,
    ) -> Result<WorkReportHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let mode = report.mode;
        let scoped = report.scoped.clone();
        let start = report.start;
        let end = report.end;
        let page = report.page;
        let export_all = report.export_all;
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                Self::access(),
                WorkReportSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let scoped = scoped.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding {
                            return Err(scope().into());
                        }
                        current.require_permission("self.tasks")?;
                        let projection = WorkReportProjection {
                            mode,
                            scoped: &scoped,
                            start,
                            end,
                            page,
                        };
                        let report = read_work_report_projection(connection, &projection, export_all)
                            .await
                            .map_err(|_| unavailable())?;
                        Ok(WorkReportHttpResponse {
                            data: native_work_report_json(&report),
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct NativeWorkReportBackend {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<NativeWorkReportService>,
}

impl NativeWorkReportBackend {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<NativeWorkReportService>,
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
        let (branch_slug, department_slug) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: CompanyId::new(
                    headers
                        .get("x-erpx-context-company")
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or_default(),
                )?,
                branch_slug,
                department_slug,
                expected: None,
                mutation: false,
            },
        ))
    }
}

impl WorkReportHttpBackend for NativeWorkReportBackend {
    fn read<'a>(
        &'a self,
        request: WorkReportHttpRequest<'a>,
    ) -> WorkReportHttpFuture<'a, WorkReportHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .read(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.projection,
                )
                .await
                .map_err(business)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_is_the_native_report_api_read() {
        let access = NativeWorkReportService::access();
        assert!(access.permission.is_satisfied_by(&["self.tasks".into()].into_iter().collect()));
        assert_eq!(access.destination, "work/native-report");
        assert!(access.api);
        assert!(access.module.is_none());
    }

    #[test]
    fn failure_mapping_keeps_business_errors_and_hides_persistence() {
        let forbidden = business(PgTransactionError::Business(scope()));
        assert_eq!(forbidden.code, "WORK_REPORT_SCOPE_REQUIRED");
        assert_eq!(forbidden.kind, FailureKind::Forbidden);
        let unavailable =
            business(PgTransactionError::Persistence(sqlx::Error::PoolClosed.into()));
        assert_eq!(unavailable.code, "WORK_REPORT_UNAVAILABLE");
        assert_eq!(unavailable.kind, FailureKind::Unavailable);
    }
}
