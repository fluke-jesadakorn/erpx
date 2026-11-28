//! Composition for the native Work home read-cursor command HTTP route.
//!
//! Standalone module: the lead mounts [`home_read_cursor_router`] into the
//! private gateway and injects the backend built by
//! [`HomeReadCursorBackend::new`]. It resolves the caller-bound Holding, opens
//! one transaction per command and injects the owner `PgHomeReadCursor`
//! adapter. No SQL and no business rules live here.
use erpx_platform_runtime_server::public::http_support::{HeaderMap, HeaderName, Router};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::sync::Arc;
use erpx_work_projection_server::public::owner_write::{
    PgOwnerWrite, execute_owner_write, is_calendar_write,
};
use erpx_work_projection_server::public::owner_write_http::{
    OwnerWriteFuture, OwnerWriteHttpBackend, OwnerWriteHttpRequest, OwnerWriteHttpTarget,
    owner_write_router,
};
use erpx_work_projection_server::public::home_read_cursor::{
    PgHomeReadCursor, mark_home_read,
};
use erpx_work_projection_server::public::home_read_cursor_http::{
    HomeReadCursorHttpBackend, HomeReadCursorHttpFuture, HomeReadCursorHttpKind,
    HomeReadCursorHttpRequest, HomeReadCursorHttpResponse, HomeReadCursorHttpTarget,
    home_read_cursor_http_router,
};

type Error = PgTransactionError<BusinessError>;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_WORK_UNAVAILABLE",
        "The native Work service is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

pub struct HomeReadCursorBackend {
    holdings: std::collections::BTreeMap<String, DatabasePool>,
}

impl HomeReadCursorBackend {
    pub fn new(holdings: impl IntoIterator<Item = (String, DatabasePool)>) -> Self {
        Self {
            holdings: holdings.into_iter().collect(),
        }
    }

    fn company(headers: &HeaderMap, expected: &str) -> Result<(), BusinessError> {
        let mut values = headers.get_all(CONTEXT_COMPANY).iter();
        let first = values.next();
        if values.next().is_some() {
            return Err(BusinessError::invalid(
                "HOME_READ_CURSOR_REQUEST_INVALID",
                "Use one bounded home read-cursor command.",
            ));
        }
        if let Some(value) = first
            && value.to_str().map(|text| text != expected).unwrap_or(true)
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "HOME_READ_CURSOR_CONTEXT_CHANGED",
                "The Company scope changed. Reload before continuing.",
            ));
        }
        Ok(())
    }
}

impl HomeReadCursorHttpBackend for HomeReadCursorBackend {
    fn command<'a>(
        &'a self,
        request: HomeReadCursorHttpRequest<'a>,
    ) -> HomeReadCursorHttpFuture<'a, HomeReadCursorHttpResponse> {
        Box::pin(async move {
            let pool = self
                .holdings
                .get(request.holding_slug)
                .ok_or_else(unavailable)?
                .clone();
            let HomeReadCursorHttpKind::Mark(input) = request.kind else {
                return Err(BusinessError::invalid(
                    "HOME_READ_CURSOR_REQUEST_INVALID",
                    "Use one bounded home read-cursor command.",
                ));
            };
            Self::company(request.headers, &input.company_id)?;
            let data = PgUnitOfWork::new(pool)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let input = input.clone();
                    Box::pin(async move {
                        let mut repository = PgHomeReadCursor::new(&mut *connection);
                        mark_home_read(&mut repository, &input).await?;
                        Ok(serde_json::json!({"ok": true}))
                    })
                })
                .await
                .map_err(business)?;
            Ok(HomeReadCursorHttpResponse { data })
        })
    }
}

/// Build the owner router bound to a backend. Lead mounts the returned Router.
pub fn home_read_cursor_router(
    host: impl Into<String>,
    holding_slug: impl Into<String>,
    origin: impl Into<String>,
    backend: std::sync::Arc<dyn HomeReadCursorHttpBackend>,
) -> Result<Router, BusinessError> {
    home_read_cursor_http_router(vec![HomeReadCursorHttpTarget {
        host: host.into(),
        holding_slug: holding_slug.into(),
        origin: origin.into(),
        backend,
    }])
}

/// Executor for the owner-wide Manage ERPX command router. It selects the
/// exact registered Holding pool or the Control pool from the route target,
/// opens one transaction and injects `PgOwnerWrite`. No SQL lives here.
pub struct WorkProjectionOwnerWriteBackend {
    reads: std::collections::BTreeMap<String, DatabasePool>,
    holdings: std::collections::BTreeMap<String, DatabasePool>,
    control: DatabasePool,
}

impl WorkProjectionOwnerWriteBackend {
    pub fn new(
        holdings: impl IntoIterator<Item = (String, DatabasePool)>,
        control: DatabasePool,
    ) -> Self {
        Self {
            reads: std::collections::BTreeMap::new(),
            holdings: holdings.into_iter().collect(),
            control,
        }
    }

    /// Work projection tables live in each Holding read model. The write pool
    /// remains a fallback only for deployments without a registered read model.
    /// Calendar commands are the exception: their SQL touches the Holding
    /// calendar tables read by the native calendar router, so they always use
    /// the exact Holding write pool.
    pub fn with_reads(
        reads: impl IntoIterator<Item = (String, DatabasePool)>,
        holdings: impl IntoIterator<Item = (String, DatabasePool)>,
        control: DatabasePool,
    ) -> Self {
        Self {
            reads: reads.into_iter().collect(),
            holdings: holdings.into_iter().collect(),
            control,
        }
    }
}

impl OwnerWriteHttpBackend for WorkProjectionOwnerWriteBackend {
    fn execute<'a>(&'a self, request: OwnerWriteHttpRequest<'a>) -> OwnerWriteFuture<'a> {
        Box::pin(async move {
            let kind = request.command.kind.clone();
            let pool = if request.control {
                self.control.clone()
            } else if is_calendar_write(&kind) {
                self.holdings
                    .get(request.holding_slug)
                    .ok_or_else(unavailable)?
                    .clone()
            } else {
                self.reads
                    .get(request.holding_slug)
                    .or_else(|| self.holdings.get(request.holding_slug))
                    .ok_or_else(unavailable)?
                    .clone()
            };
            let values = request.command.values.clone();
            let company = request.command.company_id.clone();
            let now = request.command.now;
            let key = request.key.to_owned();
            let actor = request
                .headers
                .get(CONTEXT_USER)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("owner-write")
                .to_owned();
            PgUnitOfWork::new(pool)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let kind = kind.clone();
                    let values = values.clone();
                    let company = company.clone();
                    let actor = actor.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        let mut repository = PgOwnerWrite::new(&mut *connection);
                        let command = erpx_work_projection_server::public::owner_write::OwnerWriteCommand {
                            kind: &kind,
                            values: &values,
                            company_id: &company,
                            actor_id: &actor,
                            now,
                        };
                        execute_owner_write(&mut repository, &command, &key).await
                    })
                })
                .await
                .map_err(business)
        })
    }
}

const CONTEXT_USER: erpx_platform_runtime_server::public::http_support::HeaderName =
    erpx_platform_runtime_server::public::http_support::HeaderName::from_static("x-erpx-context-user");

/// Build the owner-wide router from a configured backend and exact targets.
pub fn work_projection_owner_write_router(
    backend: Arc<dyn OwnerWriteHttpBackend>,
    targets: Vec<(String, String, String, bool)>,
) -> Result<Router, BusinessError> {
    owner_write_router(
        targets
            .into_iter()
            .map(|(host, holding_slug, origin, control)| OwnerWriteHttpTarget {
                host,
                holding_slug,
                origin,
                control,
                backend: backend.clone(),
            })
            .collect(),
    )
}
