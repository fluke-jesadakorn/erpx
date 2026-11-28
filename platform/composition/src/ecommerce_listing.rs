//! Native Ecommerce listing command composition. The owner router owns only
//! transport; Account/Holding authority and the serializable owner unit stay
//! with the coordinator, and the Commerce product/currency and Finance unit-price
//! snapshots are supplied by the producer owners behind the Ecommerce ports.
use crate::business_http::BusinessScopeSlugs;
use crate::ecommerce::EcommerceConfiguration;
use erpx_commerce_server::public::ecommerce_listing_source::{
    EcommerceListingCurrency, active_product, functional_currency,
};
use erpx_ecommerce_server::public::ecommerce_http::{
    EcommerceHttpFuture, EcommerceHttpResponse, EcommerceListingChangeHttpRequest,
    EcommerceListingCreateHttpRequest, EcommerceListingHttpBackend,
};
use erpx_ecommerce_server::public::listing::{
    ChangeListing, CreateListing, EcommerceActor, EcommerceFunctionalCurrency,
    EcommerceListingMoneyRequest, EcommerceListingMoneySnapshot, PgEcommerceListing,
    PgEcommerceListingCommerceSource, PgEcommerceListingMoneyCapture, change_listing, create_listing,
};
use erpx_finance_server::public::money_snapshot::{
    MoneySnapshotInput, lock_money_snapshot_sources, prepare_money_snapshot,
};
use erpx_identity_access_server::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_platform_runtime_server::public::http_support::HeaderMap;
use erpx_platform_runtime_server::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_platform_shared_server::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use sqlx::PgConnection;
use std::{future::Future, pin::Pin, sync::Arc};

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ECOMMERCE_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ECOMMERCE_UNAVAILABLE",
        "The Ecommerce workspace is unavailable.",
    )
}

fn money_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ECOMMERCE_MONEY_EVIDENCE_INVALID",
        "The Finance unit-price snapshot could not be retained.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

fn ecommerce_actor(current: &AuthorizedWorkContext) -> Result<EcommerceActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(EcommerceActor {
        user_id: current.user_id().clone(),
        holding_id: current.holding_id().clone(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        assignment_id: assignment.id.clone(),
        role: Some(assignment.role.clone()),
        authority_revision: Some(selected.revision()),
        assignment_version: Some(assignment.version),
        permissions: current.command_iam_metadata().permissions.clone(),
        modules: current.modules().to_vec(),
        expires_at: current.expires_at(),
    })
}

fn work_context(current: &AuthorizedWorkContext) -> serde_json::Value {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    serde_json::json!({
        "userId": current.user_id().as_str(),
        "holdingId": current.holding_id().as_str(),
        "companyId": workspace.company_id.as_str(),
        "branchId": workspace.branch_id.as_ref().map(|value| value.as_str()),
        "departmentId": workspace.department_id.as_ref().map(|value| value.as_str()),
        "branchSlug": workspace.branch_slug,
        "departmentSlug": workspace.department_slug,
        "assignmentId": assignment.id.as_str(),
        "assignmentVersion": assignment.version,
        "revision": selected.revision(),
        "role": assignment.role,
        "assignmentScopeKind": match assignment.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        },
        "modules": current.modules(),
        "permissions": current.command_iam_metadata().permissions,
        "expiresAt": current.expires_at(),
    })
}

#[derive(Clone, Copy)]
struct ListingSources;

impl HoldingWorkSourcePreparation for ListingSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

/// Commerce-owned active-product and functional-currency witness mapped onto
/// the Ecommerce producer port. No owner table is written here.
#[derive(Clone, Copy)]
struct CommerceListingSource;

impl PgEcommerceListingCommerceSource for CommerceListingSource {
    fn active_product<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        company: &'a CompanyId,
        product_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<bool, BusinessError>> + Send + 'a>> {
        Box::pin(active_product(connection, company, product_id))
    }

    fn functional_currency<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        company: &'a CompanyId,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Option<EcommerceFunctionalCurrency>, BusinessError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let configured = functional_currency(connection, company).await?;
            Ok(configured.map(
                |EcommerceListingCurrency {
                     currency,
                     base_currency,
                 }| EcommerceFunctionalCurrency {
                    currency,
                    base_currency,
                },
            ))
        })
    }
}

static COMMERCE: CommerceListingSource = CommerceListingSource;

/// A capture clock pinned to the owner command instant, so the Finance
/// evidence `captured_at` can never exceed the command's `now`.
#[derive(Clone, Copy)]
struct CommandClock(TimestampMillis);

impl Clock for CommandClock {
    fn now(&self) -> TimestampMillis {
        self.0
    }
}

/// Finance-owned `snapshotMoney` producer mapped onto the Ecommerce money
/// port. It retains the unit price on the caller's serializable unit.
#[derive(Clone, Copy)]
struct ListingMoneyCapture;

impl PgEcommerceListingMoneyCapture for ListingMoneyCapture {
    fn capture<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        _actor: &'a EcommerceActor,
        request: &'a EcommerceListingMoneyRequest,
        now: TimestampMillis,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<EcommerceListingMoneySnapshot, BusinessError>> + Send + 'a,
        >,
    > {
        Box::pin(async move {
            lock_money_snapshot_sources(connection)
                .await
                .map_err(business)?;
            let transaction_date: String =
                sqlx::query_scalar("SELECT to_char(CURRENT_DATE, 'YYYY-MM-DD')")
                    .fetch_one(&mut *connection)
                    .await
                    .map_err(|_| money_invalid())?;
            let input = MoneySnapshotInput {
                source_type: "ecommerce_listing".into(),
                source_id: request.listing_id.clone(),
                amount_kind: "unit_price".into(),
                transaction_date,
                currency: request.currency.clone(),
                currency_scale: Some(2),
                amount_units: request.amount_minor,
            };
            let prepared = prepare_money_snapshot(
                connection,
                &CompanyId::new(request.company_id.clone())?,
                input,
                &CommandClock(now),
            )
            .await
            .map_err(business)?;
            let retained = prepared.persist(connection).await.map_err(business)?;
            let evidence = retained.evidence();
            let amount_units = evidence.amount_units.parse::<i64>().map_err(|_| money_invalid())?;
            let usd_units = evidence.usd_units.parse::<i64>().map_err(|_| money_invalid())?;
            Ok(EcommerceListingMoneySnapshot {
                currency_scale: i64::from(evidence.currency_scale),
                amount_units,
                usd_units,
                rate_id: evidence.rate_id.clone(),
                used_latest_fallback: evidence.used_latest_fallback,
                captured_at: evidence.captured_at,
            })
        })
    }
}

static MONEY: ListingMoneyCapture = ListingMoneyCapture;

pub struct EcommerceListingService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: EcommerceConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> EcommerceListingService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: EcommerceConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
        }
    }

    fn access() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: "commerce.read".into(),
            module: Some("ecommerce".into()),
            destination: "ecommerce".into(),
            api: true,
        }
    }

    fn command_input<Input>(
        &self,
        input: Input,
    ) -> impl FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company().clone();
        move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = true;
            Ok(request)
        }
    }

    pub async fn create_listing<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: CreateListing,
    ) -> Result<EcommerceHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding().clone();
        let company = self.configuration.company().clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.command_input(input),
                Self::access(),
                ListingSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let actor = ecommerce_actor(current)?;
                        let now = clock.now();
                        actor.require_write(now)?;
                        let mut repository =
                            PgEcommerceListing::with_sources(connection, &COMMERCE, &MONEY);
                        let receipt =
                            create_listing(&actor, &command, &mut repository, now)
                                .await
                                .map_err(business)?;
                        Ok(EcommerceHttpResponse {
                            data: receipt.to_wire(),
                            context: work_context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn change_listing<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: ChangeListing,
    ) -> Result<EcommerceHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding().clone();
        let company = self.configuration.company().clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.command_input(input),
                Self::access(),
                ListingSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let actor = ecommerce_actor(current)?;
                        let now = clock.now();
                        actor.require_write(now)?;
                        let mut repository = PgEcommerceListing::new(connection);
                        let receipt =
                            change_listing(&actor, &command, &mut repository, now)
                                .await
                                .map_err(business)?;
                        Ok(EcommerceHttpResponse {
                            data: receipt.to_wire(),
                            context: work_context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct EcommerceListingBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<EcommerceListingService<C>>,
}

impl<C: Clock + Send + Sync + 'static> EcommerceListingBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<EcommerceListingService<C>>,
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
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.service.configuration.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: false,
            },
        ))
    }
}

impl<C: Clock + Send + Sync + 'static> EcommerceListingHttpBackend for EcommerceListingBackend<C> {
    fn create_listing<'a>(
        &'a self,
        request: EcommerceListingCreateHttpRequest<'a>,
    ) -> EcommerceHttpFuture<'a, EcommerceHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let command = CreateListing {
                key: CommandKey::new(request.key)?,
                channel_id: RecordId::new(request.channel_id)?,
                product_id: RecordId::new(request.product_id)?,
                title: request.title.to_owned(),
                currency: request.currency.to_owned(),
                price: request.price.to_owned(),
            };
            self.service
                .create_listing(request.host, proof, move || Ok(selection), command)
                .await
                .map_err(business)
        })
    }

    fn change_listing<'a>(
        &'a self,
        request: EcommerceListingChangeHttpRequest<'a>,
    ) -> EcommerceHttpFuture<'a, EcommerceHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let command = ChangeListing {
                key: CommandKey::new(request.key)?,
                id: RecordId::new(request.id)?,
                expected_version: request.expected_version,
                title: request.title.to_owned(),
            };
            self.service
                .change_listing(request.host, proof, move || Ok(selection), command)
                .await
                .map_err(business)
        })
    }
}
