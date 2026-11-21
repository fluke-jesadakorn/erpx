//! Wave-2 composition for the native Marketing command HTTP ingress.
//!
//! The owner already owns the router, use cases and Postgres adapter. This
//! module supplies an alternate fail-closed backend for the private owner
//! transport: it resolves the caller/scope from internal context headers,
//! opens one serializable Holding transaction, and dispatches to the owner use
//! cases through `PgMarketingLead`. No SQL and no business rules live here.
use erpx_server_marketing::public::lead_candidate::{
    MarketingAssetCreateInput, MarketingAssetReviewInput, MarketingCampaignCreateInput,
    MarketingLeadActionInput, MarketingLeadActor, MarketingLeadCandidateInput, PgMarketingLead,
    create_asset, create_campaign, create_candidate, record_engagement, review_asset,
};
use erpx_server_marketing::public::lead_candidate_command_http::{
    MarketingAssetReviewHttpRequest, MarketingCommandHttpRequest, MarketingLeadCommandHttpBackend,
    MarketingLeadCommandHttpFuture, MarketingLeadCommandHttpRequest,
    MarketingLeadCommandHttpTarget, marketing_lead_command_http_router,
};
use erpx_server_runtime::public::http_support::{HeaderMap, HeaderName, Router};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, RecordId};
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");
const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_ASSIGNMENT: HeaderName = HeaderName::from_static("x-erpx-context-assignment");
const CONTEXT_ASSIGNMENT_VERSION: HeaderName =
    HeaderName::from_static("x-erpx-context-assignment-version");
const CONTEXT_REVISION: HeaderName = HeaderName::from_static("x-erpx-context-revision");
const CONTEXT_ROLE: HeaderName = HeaderName::from_static("x-erpx-context-role");
const CONTEXT_PERMISSIONS: HeaderName = HeaderName::from_static("x-erpx-context-permissions");
const CONTEXT_MODULES: HeaderName = HeaderName::from_static("x-erpx-context-modules");
const CONTEXT_EXPIRES_AT: HeaderName = HeaderName::from_static("x-erpx-context-expires-at");
const CONTEXT_BRANCH: HeaderName = HeaderName::from_static("x-erpx-context-branch-id");
const CONTEXT_DEPARTMENT: HeaderName = HeaderName::from_static("x-erpx-context-department-id");

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MARKETING_COMMAND_UNAVAILABLE",
        "The native Marketing command service is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "MARKETING_COMMAND_REQUEST_INVALID",
        "Use one bounded Marketing command.",
    )
}

/// Composition-owned backend for the private owner transport.
pub struct MarketingWave2Backend<C> {
    holding: DatabasePool,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> MarketingWave2Backend<C> {
    pub fn new(holding: DatabasePool, clock: Arc<C>) -> Self {
        Self { holding, clock }
    }

    fn header<'a>(headers: &'a HeaderMap, name: HeaderName) -> Result<&'a str, BusinessError> {
        let mut values = headers.get_all(name).iter();
        let first = values.next();
        if values.next().is_some() {
            return Err(request_invalid());
        }
        first
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.is_empty())
            .ok_or_else(request_invalid)
    }

    fn number(headers: &HeaderMap, name: HeaderName) -> Result<i64, BusinessError> {
        Self::header(headers, name)?
            .parse::<i64>()
            .map_err(|_| request_invalid())
    }

    fn optional_id(
        headers: &HeaderMap,
        name: HeaderName,
    ) -> Result<Option<RecordId>, BusinessError> {
        let value = Self::header(headers, name)?;
        if value == "-" {
            return Ok(None);
        }
        RecordId::new(value)
            .map(Some)
            .map_err(|_| request_invalid())
    }

    fn actor(headers: &HeaderMap) -> Result<MarketingLeadActor, BusinessError> {
        let assignment_id = Self::header(headers, CONTEXT_ASSIGNMENT)?;
        let assignment_version = Self::number(headers, CONTEXT_ASSIGNMENT_VERSION)?;
        let revision = Self::number(headers, CONTEXT_REVISION)?;
        let expires_at = Self::number(headers, CONTEXT_EXPIRES_AT)?;
        let permissions = Self::header(headers, CONTEXT_PERMISSIONS)?
            .split(',')
            .map(str::to_owned)
            .collect();
        let modules = Self::header(headers, CONTEXT_MODULES)?
            .split(',')
            .map(str::to_owned)
            .collect();
        Ok(MarketingLeadActor {
            user_id: Self::header(headers, CONTEXT_USER)?.to_owned(),
            company_id: Self::header(headers, CONTEXT_COMPANY)?.to_owned(),
            branch_id: Self::optional_id(headers, CONTEXT_BRANCH)?,
            department_id: Self::optional_id(headers, CONTEXT_DEPARTMENT)?,
            assignment_id: assignment_id.to_owned(),
            role: Some(Self::header(headers, CONTEXT_ROLE)?.to_owned()),
            authority_revision: Some(revision),
            assignment_version: Some(assignment_version),
            permissions,
            modules,
            expires_at,
        })
    }
}

impl<C: Clock + Send + Sync + 'static> MarketingLeadCommandHttpBackend
    for MarketingWave2Backend<C>
{
    fn create_campaign<'a>(
        &'a self,
        request: MarketingCommandHttpRequest<'a>,
    ) -> MarketingLeadCommandHttpFuture<'a> {
        Box::pin(async move {
            let actor = Self::actor(request.headers)?;
            let input: MarketingCampaignCreateInput =
                serde_json::from_value(request.body.clone()).map_err(|_| request_invalid())?;
            let key = request.key.as_str().to_owned();
            let clock = self.clock.clone();
            let holding = self.holding.clone();
            let value = PgUnitOfWork::new(holding)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let actor = actor.clone();
                    let input = input.clone();
                    let key = key.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        let receipt = create_campaign(
                            &actor,
                            &input,
                            &key,
                            &mut PgMarketingLead,
                            connection,
                            clock.now(),
                        )
                        .await
                        .map_err(business)?;
                        Ok(serde_json::json!({"data": receipt}))
                    })
                })
                .await
                .map_err(business)?;
            Ok(value)
        })
    }

    fn create_candidate<'a>(
        &'a self,
        request: MarketingCommandHttpRequest<'a>,
    ) -> MarketingLeadCommandHttpFuture<'a> {
        Box::pin(async move {
            let actor = Self::actor(request.headers)?;
            let input: MarketingLeadCandidateInput =
                serde_json::from_value(request.body.clone()).map_err(|_| request_invalid())?;
            let key = request.key.as_str().to_owned();
            let clock = self.clock.clone();
            let holding = self.holding.clone();
            let value = PgUnitOfWork::new(holding)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let actor = actor.clone();
                    let input = input.clone();
                    let key = key.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        let receipt = create_candidate(
                            &actor,
                            &input,
                            &key,
                            &mut PgMarketingLead,
                            connection,
                            clock.now(),
                        )
                        .await
                        .map_err(business)?;
                        Ok(serde_json::json!({"data": receipt}))
                    })
                })
                .await
                .map_err(business)?;
            Ok(value)
        })
    }

    fn create_asset<'a>(
        &'a self,
        request: MarketingCommandHttpRequest<'a>,
    ) -> MarketingLeadCommandHttpFuture<'a> {
        Box::pin(async move {
            let actor = Self::actor(request.headers)?;
            let input: MarketingAssetCreateInput =
                serde_json::from_value(request.body.clone()).map_err(|_| request_invalid())?;
            let key = request.key.as_str().to_owned();
            let clock = self.clock.clone();
            let holding = self.holding.clone();
            let value = PgUnitOfWork::new(holding)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let actor = actor.clone();
                    let input = input.clone();
                    let key = key.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        let receipt = create_asset(
                            &actor,
                            &input,
                            &key,
                            &mut PgMarketingLead,
                            connection,
                            clock.now(),
                        )
                        .await
                        .map_err(business)?;
                        Ok(serde_json::json!({"data": receipt}))
                    })
                })
                .await
                .map_err(business)?;
            Ok(value)
        })
    }

    fn review_asset<'a>(
        &'a self,
        request: MarketingAssetReviewHttpRequest<'a>,
    ) -> MarketingLeadCommandHttpFuture<'a> {
        Box::pin(async move {
            let actor = Self::actor(request.headers)?;
            let input: MarketingAssetReviewInput =
                serde_json::from_value(request.body.clone()).map_err(|_| request_invalid())?;
            let asset_id = request.asset_id.to_owned();
            let key = request.key.as_str().to_owned();
            let clock = self.clock.clone();
            let holding = self.holding.clone();
            let value = PgUnitOfWork::new(holding)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let actor = actor.clone();
                    let input = input.clone();
                    let asset_id = asset_id.clone();
                    let key = key.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        let receipt = review_asset(
                            &actor,
                            &asset_id,
                            &input,
                            &key,
                            &mut PgMarketingLead,
                            connection,
                            clock.now(),
                        )
                        .await
                        .map_err(business)?;
                        Ok(serde_json::json!({"data": receipt}))
                    })
                })
                .await
                .map_err(business)?;
            Ok(value)
        })
    }

    fn record_engagement<'a>(
        &'a self,
        request: MarketingLeadCommandHttpRequest<'a>,
    ) -> MarketingLeadCommandHttpFuture<'a> {
        Box::pin(async move {
            let actor = Self::actor(request.headers)?;
            let input: MarketingLeadActionInput =
                serde_json::from_value(request.body.clone()).map_err(|_| request_invalid())?;
            let candidate_id = request.candidate_id.to_owned();
            let key = request.key.as_str().to_owned();
            let clock = self.clock.clone();
            let holding = self.holding.clone();
            let value = PgUnitOfWork::new(holding)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let actor = actor.clone();
                    let input = input.clone();
                    let candidate_id = candidate_id.clone();
                    let key = key.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        let receipt = record_engagement(
                            &actor,
                            &candidate_id,
                            &input,
                            &key,
                            &mut PgMarketingLead,
                            connection,
                            clock.now(),
                        )
                        .await
                        .map_err(business)?;
                        Ok(serde_json::json!({"data": receipt}))
                    })
                })
                .await
                .map_err(business)?;
            Ok(value)
        })
    }
}

/// Build the owner router bound to this backend. Lead mounts the returned Router.
pub fn marketing_wave2_router(
    host: impl Into<String>,
    holding_slug: impl Into<String>,
    origin: impl Into<String>,
    backend: Arc<dyn MarketingLeadCommandHttpBackend>,
) -> Result<Router, BusinessError> {
    marketing_lead_command_http_router(vec![MarketingLeadCommandHttpTarget {
        host: host.into(),
        holding_slug: holding_slug.into(),
        origin: origin.into(),
        backend,
    }])
}
