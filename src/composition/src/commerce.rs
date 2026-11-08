//! Native Commerce order HTTP mount. Current Account/Holding authority remains
//! with the selected coordinator; owner ports supply quotation and sales-order
//! facts.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_commerce::public::order::PgCommerceOrders;
use erpx_server_commerce::public::order::{
    AuditEntry, CommerceFulfillmentRepository, CommerceOrderActor, CommerceOrderCommandRepository,
    CommerceOrderFuture, ConvertedReadback, OrderCommandReceipt, OutboxEntry, ReceiptEntry,
    SalesOrderLineRow, SalesOrderRow, SaveOrderCommand, SaveOrderInput, ServiceCompletionCommand,
    ServiceCompletionLineInput, TransitionCommand, TransitionPlan, fulfill_service_order,
    order_detail, save_order_outcome, transition_order_guarded,
};
use erpx_server_commerce::public::order_http::ServiceCompletionInput;
use erpx_server_commerce::public::order_http::{
    CommerceOrderHttpBackend, CommerceOrderHttpCommand, CommerceOrderHttpFuture,
    CommerceOrderHttpRequest, CommerceOrderHttpResponse, CommerceOrderHttpTarget,
};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::{CapturedHoldingMutation, parse_inbound_account_request},
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, RecordId, TimestampMillis,
};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

/// Commerce repository wrapper that keeps the owner row adapter unchanged while
/// supplying the real Finance/Inventory cross-owner guard ports on the same
/// serializable connection.
struct ComposedCommerceOrders<'c> {
    inner: PgCommerceOrders<'c>,
}

impl<'c> ComposedCommerceOrders<'c> {
    fn new(connection: &'c mut PgConnection) -> Self {
        Self {
            inner: PgCommerceOrders::new(connection),
        }
    }
}

impl CommerceOrderCommandRepository for ComposedCommerceOrders<'_> {
    type Error = Error;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a CommerceOrderActor,
        write: bool,
        approve: bool,
        now: TimestampMillis,
    ) -> CommerceOrderFuture<'a, bool, Self::Error> {
        self.inner.current_authority(actor, write, approve, now)
    }

    fn lock_command<'a>(&'a mut self, key: &'a str) -> CommerceOrderFuture<'a, (), Self::Error> {
        self.inner.lock_command(key)
    }

    fn digest<'a>(&'a mut self, value: &'a str) -> CommerceOrderFuture<'a, String, Self::Error> {
        self.inner.digest(value)
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> CommerceOrderFuture<'a, Option<OrderCommandReceipt>, Self::Error> {
        self.inner.receipt(key)
    }

    fn lock_order<'a>(
        &'a mut self,
        actor: &'a CommerceOrderActor,
        id: &'a str,
    ) -> CommerceOrderFuture<'a, Option<SalesOrderRow>, Self::Error> {
        self.inner.lock_order(actor, id)
    }

    fn count_fulfillments<'a>(
        &'a mut self,
        order_id: &'a str,
    ) -> CommerceOrderFuture<'a, i64, Self::Error> {
        self.inner.count_fulfillments(order_id)
    }

    fn transition<'a>(
        &'a mut self,
        plan: &'a TransitionPlan,
    ) -> CommerceOrderFuture<'a, bool, Self::Error> {
        self.inner.transition(plan)
    }

    fn insert_converted_order<'a>(
        &'a mut self,
        plan: &'a TransitionPlan,
    ) -> CommerceOrderFuture<'a, bool, Self::Error> {
        self.inner.insert_converted_order(plan)
    }

    fn copy_converted_lines<'a>(
        &'a mut self,
        quote_id: &'a str,
        sales_id: &'a str,
    ) -> CommerceOrderFuture<'a, u64, Self::Error> {
        self.inner.copy_converted_lines(quote_id, sales_id)
    }

    fn converted_readback<'a>(
        &'a mut self,
        sales_id: &'a str,
        company: &'a CompanyId,
    ) -> CommerceOrderFuture<'a, Option<ConvertedReadback>, Self::Error> {
        self.inner.converted_readback(sales_id, company)
    }

    fn readback_order<'a>(
        &'a mut self,
        actor: &'a CommerceOrderActor,
        id: &'a str,
    ) -> CommerceOrderFuture<'a, Option<SalesOrderRow>, Self::Error> {
        self.inner.readback_order(actor, id)
    }

    fn sales_order_credit_allows<'a>(
        &'a mut self,
        company: &'a CompanyId,
        party_id: &'a str,
        order_total_minor: i64,
    ) -> CommerceOrderFuture<'a, bool, Self::Error> {
        let connection = self.inner.connection();
        Box::pin(async move {
            erpx_server_finance::public::sales_order_credit::sales_order_credit_allows(
                connection,
                company,
                party_id,
                order_total_minor,
            )
            .await
        })
    }

    fn release_confirmed_sales_reservations<'a>(
        &'a mut self,
        company: &'a CompanyId,
        order_id: &'a str,
    ) -> CommerceOrderFuture<'a, (), Self::Error> {
        let connection = self.inner.connection();
        Box::pin(async move {
            erpx_server_inventory::public::commerce_order_cancellation::release_confirmed_sales_order_reservations(
                connection,
                company,
                order_id,
            )
            .await
            .map(|_| ())
        })
    }

    fn insert_receipt<'a>(
        &'a mut self,
        entry: &'a ReceiptEntry<'a>,
        now: TimestampMillis,
    ) -> CommerceOrderFuture<'a, bool, Self::Error> {
        self.inner.insert_receipt(entry, now)
    }

    fn audit<'a>(
        &'a mut self,
        actor: &'a CommerceOrderActor,
        entry: &'a AuditEntry<'a>,
        now: TimestampMillis,
    ) -> CommerceOrderFuture<'a, (), Self::Error> {
        self.inner.audit(actor, entry, now)
    }

    fn outbox<'a>(
        &'a mut self,
        entry: &'a OutboxEntry<'a>,
        now: TimestampMillis,
    ) -> CommerceOrderFuture<'a, (), Self::Error> {
        self.inner.outbox(entry, now)
    }
}

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "COMMERCE_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMMERCE_UNAVAILABLE",
        "The Commerce workspace is unavailable.",
    )
}

fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCE_REQUEST_INVALID",
        "The Commerce request is invalid.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct CommerceSources {
    money: bool,
}

impl HoldingWorkSourcePreparation for CommerceSources {
    fn prepare<'c>(
        &'c self,
        connection: &'c mut PgConnection,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            if self.money {
                erpx_server_finance::public::commerce_order_money::lock_commerce_order_money_sources(connection).await?;
            }
            Ok(())
        })
    }
}

#[derive(Clone)]
pub struct CommerceConfiguration {
    holding: HoldingId,
    companies: Vec<CompanyId>,
}

impl CommerceConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self {
            holding,
            companies: vec![company],
        }
    }

    pub fn for_companies(
        holding: HoldingId,
        companies: Vec<CompanyId>,
    ) -> Result<Self, BusinessError> {
        if companies.is_empty()
            || companies
                .iter()
                .enumerate()
                .any(|(i, c)| companies[..i].contains(c))
        {
            return Err(scope());
        }
        Ok(Self { holding, companies })
    }
}

fn commerce_actor(current: &AuthorizedWorkContext) -> Result<CommerceOrderActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(CommerceOrderActor {
        user_id: current.user_id().clone(),
        person_user_ids: current.command_iam_metadata().person_user_ids.clone(),
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

fn context(current: &AuthorizedWorkContext) -> Value {
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

pub struct CommerceService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: CommerceConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> CommerceService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: CommerceConfiguration,
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
            module: Some("sales".into()),
            destination: "commerce".into(),
            api: true,
        }
    }

    fn input<Input>(
        &self,
        input: Input,
        mutation: bool,
    ) -> impl FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let companies = self.configuration.companies.clone();
        move || {
            let request = input()?;
            if !companies.contains(&request.company_id) || request.mutation != mutation {
                return Err(scope());
            }
            Ok(request)
        }
    }

    pub async fn detail<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        order_id: String,
    ) -> Result<CommerceOrderHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let companies = self.configuration.companies.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input, false),
                Self::access(),
                CommerceSources { money: false },
                move |connection, current| {
                    let holding = holding.clone();
                    let companies = companies.clone();
                    let order_id = order_id.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || !companies.contains(&current.selection().workspace().company_id)
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let actor = commerce_actor(current)?;
                        let mut repository = PgCommerceOrders::new(connection);
                        let now = clock.now();
                        let data = order_detail(&actor, &order_id, &mut repository, now).await?;
                        Ok(CommerceOrderHttpResponse {
                            data: data.to_wire(),
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn transition<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        order_id: String,
        idempotency_key: String,
        action: erpx_server_commerce::public::order::OrderAction,
        expected_version: i64,
        reason: Option<String>,
    ) -> Result<CommerceOrderHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let companies = self.configuration.companies.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input, true),
                Self::access(),
                CommerceSources { money: false },
                move |connection, current| {
                    let holding = holding.clone();
                    let companies = companies.clone();
                    let order_id = order_id.clone();
                    let idempotency_key = idempotency_key.clone();
                    let action = action;
                    let expected_version = expected_version;
                    let reason = reason.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || !companies.contains(&current.selection().workspace().company_id)
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let actor = commerce_actor(current)?;
                        let order_record =
                            RecordId::new(&order_id).map_err(|_| request_invalid())?;
                        let command_key =
                            CommandKey::new(&idempotency_key).map_err(|_| request_invalid())?;
                        let command = TransitionCommand {
                            key: command_key,
                            order_id: order_record,
                            action,
                            expected_version,
                            reason,
                        };
                        let mut repository = ComposedCommerceOrders::new(connection);
                        let now = clock.now();
                        let receipt =
                            transition_order_guarded(&actor, &command, &mut repository, now)
                                .await?;
                        Ok(CommerceOrderHttpResponse {
                            data: receipt.to_wire(),
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn fulfill<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        order_id: String,
        idempotency_key: String,
        completion: ServiceCompletionInput,
    ) -> Result<CommerceOrderHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let companies = self.configuration.companies.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input, true),
                Self::access(),
                CommerceSources { money: true },
                move |connection, current| {
                    let holding = holding.clone();
                    let companies = companies.clone();
                    let order_id = order_id.clone();
                    let idempotency_key = idempotency_key.clone();
                    let completion = completion.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || !companies.contains(&current.selection().workspace().company_id)
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let actor = commerce_actor(current)?;
                        let order_record = RecordId::new(&order_id).map_err(|_| request_invalid())?;
                        let command_key =
                            CommandKey::new(&idempotency_key).map_err(|_| request_invalid())?;
                        let date = completion.date.clone();
                        let command = ServiceCompletionCommand {
                            key: command_key,
                            order_id: order_record,
                            date: completion.date,
                            evidence_note: completion.evidence_note,
                            expected_version: completion.expected_version,
                            lines: completion
                                .lines
                                .into_iter()
                                .map(|line| ServiceCompletionLineInput {
                                    line_id: line.line_id,
                                    quantity: line.quantity,
                                })
                                .collect(),
                        };
                        let now = clock.now();
                        let mut repository = PgCommerceOrders::new(connection);
                        let outcome =
                            fulfill_service_order(&actor, &command, &mut repository, now).await?;
                        if outcome.created {
                            let money_lines = repository
                                .fulfillment_money_lines(&outcome.receipt.fulfillment_id)
                                .await?;
                            let currency = repository
                                .order_currency(&order_id)
                                .await?
                                .ok_or_else(not_found)?;
                            drop(repository);
                            let money = money_lines
                                .into_iter()
                                .map(|(id, subtotal, tax)| {
                                    erpx_server_finance::public::commerce_order_money::CommerceFulfillmentMoneyLine {
                                        id,
                                        subtotal,
                                        tax,
                                    }
                                })
                                .collect::<Vec<_>>();
                            erpx_server_finance::public::commerce_order_money::snapshot_commerce_fulfillment_money(
                                connection,
                                &actor.company_id,
                                &date,
                                &currency,
                                &money,
                                now,
                            )
                            .await?;
                        }
                        Ok(CommerceOrderHttpResponse {
                            data: outcome.receipt.to_wire(),
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn save<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        order_id: Option<String>,
        idempotency_key: String,
        save_input: SaveOrderInput,
    ) -> Result<CommerceOrderHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let companies = self.configuration.companies.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input, true),
                Self::access(),
                CommerceSources { money: true },
                move |connection, current| {
                    let holding = holding.clone();
                    let companies = companies.clone();
                    let order_id = order_id.clone();
                    let idempotency_key = idempotency_key.clone();
                    let save_input = save_input.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || !companies.contains(&current.selection().workspace().company_id)
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let actor = commerce_actor(current)?;
                        let order_record = order_id
                            .as_deref()
                            .map(RecordId::new)
                            .transpose()
                            .map_err(|_| request_invalid())?;
                        let command_key =
                            CommandKey::new(&idempotency_key).map_err(|_| request_invalid())?;
                        let command = SaveOrderCommand {
                            key: command_key,
                            order_id: order_record,
                            input: save_input,
                        };
                        let now = clock.now();
                        let outcome = {
                            let mut repository = PgCommerceOrders::new(connection);
                            save_order_outcome(&actor, &command, &mut repository, now).await?
                        };
                        // A replayed receipt already wrote its Finance evidence in
                        // the winning invocation; only a fresh plan needs snapshots.
                        if let Some(plan) = outcome.plan.as_ref() {
                            let money = erpx_server_finance::public::commerce_order_money::CommerceOrderMoneyInput {
                                order_id: plan.id.clone(),
                                version: plan.version,
                                transaction_date: plan.order_date.clone(),
                                currency: plan.currency.clone(),
                                total: plan.subtotal_minor + plan.tax_minor,
                                lines: plan
                                    .lines
                                    .iter()
                                    .map(|line| {
                                        erpx_server_finance::public::commerce_order_money::CommerceOrderMoneyLine {
                                            id: line.id.clone(),
                                            unit_price: line.price_minor,
                                            subtotal: line.subtotal_minor,
                                            tax: line.tax_minor,
                                        }
                                    })
                                    .collect(),
                            };
                            erpx_server_finance::public::commerce_order_money::snapshot_commerce_order_revision(
                                connection,
                                &actor.company_id,
                                &money,
                                now,
                            )
                            .await?;
                        }
                        Ok(CommerceOrderHttpResponse {
                            data: outcome.receipt.to_wire(),
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct CommerceBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<CommerceService<C>>,
}

impl<C: Clock + Send + Sync + 'static> CommerceBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<CommerceService<C>>,
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
    ) -> Result<(DeviceProof, Result<CapturedHoldingMutation, BusinessError>), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found());
        }
        let selection = self.scopes.slugs(headers).map(|(branch, department)| {
            CapturedHoldingMutation::capture(headers, branch, department)
        });
        Ok((inbound.into_proof(), selection))
    }
}

impl<C: Clock + Send + Sync + 'static> CommerceOrderHttpBackend for CommerceBackend<C> {
    fn execute<'a>(
        &'a self,
        request: CommerceOrderHttpRequest<'a>,
    ) -> CommerceOrderHttpFuture<'a, CommerceOrderHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            match request.command {
                CommerceOrderHttpCommand::Detail { order_id } => self
                    .service
                    .detail(
                        request.host,
                        proof,
                        move || selection?.resolve_rendered_read(),
                        order_id,
                    )
                    .await
                    .map_err(business),
                CommerceOrderHttpCommand::Transition {
                    order_id,
                    idempotency_key,
                    action,
                    expected_version,
                    reason,
                } => self
                    .service
                    .transition(
                        request.host,
                        proof,
                        move || selection?.resolve(),
                        order_id,
                        idempotency_key,
                        action,
                        expected_version,
                        reason,
                    )
                    .await
                    .map_err(business),
                CommerceOrderHttpCommand::Save {
                    order_id,
                    idempotency_key,
                    input,
                } => self
                    .service
                    .save(
                        request.host,
                        proof,
                        move || selection?.resolve(),
                        order_id,
                        idempotency_key,
                        input,
                    )
                    .await
                    .map_err(business),
                CommerceOrderHttpCommand::Fulfill {
                    order_id,
                    idempotency_key,
                    input,
                } => self
                    .service
                    .fulfill(
                        request.host,
                        proof,
                        move || selection?.resolve(),
                        order_id,
                        idempotency_key,
                        input,
                    )
                    .await
                    .map_err(business),
            }
        })
    }
}

#[allow(dead_code)]
fn _router(targets: Vec<CommerceOrderHttpTarget>) -> Result<axum::Router, BusinessError> {
    erpx_server_commerce::public::order_http::commerce_order_http_router(targets)
}
