//! Private, current-authority Service reservation readback.
use crate::field_service_parts::FieldServicePartsEnvironment;
use erpx_server_commerce::public::field_service_reservation::{
    FieldServiceStockScopeRequest, lock_field_service_stock_scope_sources,
    read_current_field_service_stock_scope,
};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, lock_commercial_authority_sources,
};
use erpx_server_field_service::public::reservation_read::{
    FieldServiceIssueEvent, FieldServiceReservationRead, FieldServiceReserveEvent,
    FieldServiceReserveResult, FieldServiceStockSnapshot, FieldServiceVisitEvent,
    lock_field_service_reservation_read_sources, read_field_service_issue,
    read_field_service_reservation, read_field_service_visit_by_key,
};
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_inventory::public::field_service_stock_read::{
    lock_field_service_stock_read_sources, read_field_service_stock,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, FailureKind, HoldingId, RecordId, TimestampMillis,
};
use erpx_server_workflow::public::record_origin::lock_record_origin_sources;
use serde::Serialize;
use sqlx::PgConnection;
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;
fn denied() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FIELD_SERVICE_CONFIGURATION_SCOPE_CHANGED",
        "The configured Field Service scope changed.",
    )
}
fn safe(value: i64) -> Result<i64, BusinessError> {
    if !(-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&value) {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "FIELD_SERVICE_READ_LIMIT",
            "The Service source exceeds this bounded read.",
        ));
    }
    Ok(value)
}
pub(crate) struct ReplyBudget(pub(crate) usize);
impl std::io::Write for ReplyBudget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .filter(|n| *n <= 2 * 1024 * 1024)
            .ok_or_else(|| std::io::Error::other("bounded Service reply"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceRenderedContext {
    pub user_id: String,
    pub holding_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub branch_slug: Option<String>,
    pub department_slug: Option<String>,
    pub assignment_id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature="contracts",schema(value_type=i64,minimum=0,maximum=9_007_199_254_740_991_i64))]
    pub assignment_version: i64,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature="contracts",schema(value_type=i64,minimum=0,maximum=9_007_199_254_740_991_i64))]
    pub revision: i64,
    pub role: String,
    pub assignment_scope_kind: String,
    pub modules: Vec<String>,
    pub permissions: Vec<String>,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature="contracts",schema(value_type=i64,minimum=0,maximum=9_007_199_254_740_991_i64))]
    pub expires_at: i64,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceReservationReadResponse {
    pub context: FieldServiceRenderedContext,
    pub order: FieldServiceReserveResult,
    pub receipt: FieldServiceReserveResult,
    pub event: FieldServiceReserveEvent,
    pub reservation: FieldServiceReservationRead,
    pub stock: FieldServiceStockSnapshot,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceIssueReadResponse {
    pub context: FieldServiceRenderedContext,
    pub order: FieldServiceReserveResult,
    pub receipt: FieldServiceReserveResult,
    pub event: FieldServiceIssueEvent,
    pub reservation: FieldServiceReservationRead,
    pub stock: FieldServiceStockSnapshot,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceVisitReadResponse {
    pub context: FieldServiceRenderedContext,
    pub order: FieldServiceReserveResult,
    pub receipt: FieldServiceReserveResult,
    pub event: FieldServiceVisitEvent,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceAgreementReviewReadResponse {
    pub context: FieldServiceRenderedContext,
    pub receipt: FieldServiceReserveResult,
    pub agreement: FieldServiceReserveResult,
    pub event:
        erpx_server_field_service::public::reservation_read::FieldServiceAgreementReviewEvent,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceAgreementCreateReadResponse {
    pub context: FieldServiceRenderedContext,
    pub receipt: FieldServiceReserveResult,
    pub agreement: FieldServiceReserveResult,
    pub event:
        erpx_server_field_service::public::reservation_read::FieldServiceAgreementReviewEvent,
    pub money: erpx_server_finance::public::field_service_agreement_money::AgreementMoneyEvidence,
}
/// Request verification separates immutable create evidence from current state.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceRequestReadResponse {
    pub context: FieldServiceRenderedContext,
    pub receipt: erpx_server_field_service::public::reservation_read::FieldServiceRequestResult,
    pub order: erpx_server_field_service::public::reservation_read::FieldServiceRequestOrderRead,
    pub ticket: erpx_server_field_service::public::reservation_read::FieldServiceRequestTicketRead,
    pub event: erpx_server_field_service::public::reservation_read::FieldServiceRequestEvent,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceOrderCreateReadResponse {
    pub context: FieldServiceRenderedContext,
    pub receipt: FieldServiceReserveResult,
    pub order: erpx_server_field_service::public::reservation_read::FieldServiceCreatedOrderRead,
    pub ticket: erpx_server_field_service::public::reservation_read::FieldServiceRequestTicketRead,
    pub event: erpx_server_field_service::public::reservation_read::FieldServiceRequestEvent,
}
pub(crate) fn context(
    current: &AuthorizedWorkContext,
) -> Result<FieldServiceRenderedContext, BusinessError> {
    let selected = current.selection();
    let scope = selected.workspace();
    let grant = selected.assignment();
    Ok(FieldServiceRenderedContext {
        user_id: current.user_id().as_str().into(),
        holding_id: current.holding_id().as_str().into(),
        company_id: scope.company_id.as_str().into(),
        branch_id: scope.branch_id.as_ref().map(|v| v.as_str().into()),
        department_id: scope.department_id.as_ref().map(|v| v.as_str().into()),
        branch_slug: scope.branch_slug.clone(),
        department_slug: scope.department_slug.clone(),
        assignment_id: grant.id.as_str().into(),
        assignment_version: safe(grant.version)?,
        revision: safe(selected.revision())?,
        role: grant.role.clone(),
        assignment_scope_kind: match grant.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .into(),
        modules: current.modules().to_vec(),
        permissions: current.command_iam_metadata().permissions.clone(),
        expires_at: safe(current.expires_at())?,
    })
}
#[derive(Clone)]
struct ReadSources {
    issues: bool,
}
#[derive(Clone)]
struct AgreementReadSources;
impl HoldingWorkSourcePreparation for AgreementReadSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_record_origin_sources(c).await?;
            erpx_server_field_service::public::reservation_read::lock_agreement_review_read_sources(
                c,
            )
            .await
        })
    }
}
#[derive(Clone)]
struct AgreementCreateReadSources;
impl HoldingWorkSourcePreparation for AgreementCreateReadSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            erpx_server_finance::public::field_service_agreement_money::lock_agreement_money_read_sources(c).await?;
            AgreementReadSources.prepare(c).await
        })
    }
}
impl HoldingWorkSourcePreparation for ReadSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_record_origin_sources(c).await?;
            lock_field_service_reservation_read_sources(c).await?;
            lock_field_service_stock_scope_sources(c).await?;
            lock_field_service_stock_read_sources(c).await?;
            if self.issues {
                erpx_server_inventory::public::field_service_consumption_finance::lock_consumption_finance_sources(c).await?;
            }
            Ok(())
        })
    }
}
#[derive(Clone)]
pub struct FieldServiceReservationReadConfiguration {
    holding: HoldingId,
    company: CompanyId,
    catalogue: Arc<CommercialCatalogue>,
    issues: bool,
    requests: bool,
    order_creates: bool,
    visits: bool,
    completions: bool,
    recoveries: bool,
    cancellations: bool,
    agreement_reviews: bool,
    agreement_creates: bool,
}
impl FieldServiceReservationReadConfiguration {
    pub fn new(
        environment: FieldServicePartsEnvironment,
        holding: HoldingId,
        company: CompanyId,
        catalogue: Arc<CommercialCatalogue>,
    ) -> Result<Self, BusinessError> {
        if matches!(environment, FieldServicePartsEnvironment::Production) {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "FIELD_SERVICE_PRODUCTION_COMPOSITION_UNAVAILABLE",
                "Field Service production release is not available.",
            ));
        }
        Ok(Self {
            holding,
            company,
            catalogue,
            issues: false,
            requests: false,
            order_creates: false,
            visits: false,
            completions: false,
            recoveries: false,
            cancellations: false,
            agreement_reviews: false,
            agreement_creates: false,
        })
    }
    pub fn with_cancellations(mut self) -> Self {
        self.cancellations = true;
        self
    }
    pub fn with_agreement_reviews(mut self) -> Self {
        self.agreement_reviews = true;
        self
    }
    pub fn with_agreement_creates(mut self) -> Self {
        self.agreement_creates = true;
        self
    }
    pub fn with_recoveries(mut self) -> Self {
        self.recoveries = true;
        self
    }
    pub fn with_completions(mut self) -> Self {
        self.completions = true;
        self
    }
    pub fn with_visits(mut self) -> Self {
        self.visits = true;
        self
    }
    pub fn with_order_creates(mut self) -> Self {
        self.order_creates = true;
        self
    }
    pub fn with_requests(mut self) -> Self {
        self.requests = true;
        self
    }
    pub fn with_issues(mut self) -> Self {
        self.issues = true;
        self
    }
    pub fn company_id(&self) -> &CompanyId {
        &self.company
    }
}
pub struct FieldServiceReservationReadService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    config: FieldServiceReservationReadConfiguration,
    clock: Arc<C>,
}
impl<C: Clock + Send + Sync + 'static> FieldServiceReservationReadService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        config: FieldServiceReservationReadConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            config,
            clock,
        }
    }
    pub fn company_id(&self) -> &CompanyId {
        &self.config.company
    }
    pub async fn read<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        order: String,
        reservation: String,
    ) -> Result<FieldServiceReservationReadResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.config.company.clone();
        let input = move || {
            let mut selection = input()?;
            if selection.company_id != company {
                return Err(denied());
            }
            selection.mutation = false;
            Ok(selection)
        };
        let config = self.config.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                WorkAuthorizationRequest {
                    permission: "records.read".into(),
                    module: Some("field_service".into()),
                    destination: "service/orders".into(),
                    api: true,
                },
                ReadSources { issues: false },
                move |c, current| {
                    let config = config.clone();
                    let clock = clock.clone();
                    let order = order.clone();
                    let reservation = reservation.clone();
                    Box::pin(async move {
                        if current.holding_id() != &config.holding
                            || current.selection().workspace().company_id != config.company
                        {
                            return Err(denied().into());
                        }
                        current.require_permission("records.read")?;
                        let purchase = PgCompanyPurchaseReader::new(
                            config.holding.clone(),
                            config.company.clone(),
                            c,
                            &config.catalogue,
                        )
                        .inspect(
                            &PurchaseRequest {
                                capabilities: vec![
                                    "service.orders".into(),
                                    "service.parts-assets".into(),
                                ],
                                purpose: CapabilityPurpose::History,
                                mode: PackageEnforcementMode::Enforced,
                            },
                            clock.now(),
                        )
                        .await?;
                        purchase.require_allowed()?;
                        let source = read_field_service_reservation(
                            c,
                            current,
                            order.as_str(),
                            reservation.as_str(),
                        )
                        .await?;
                        let part = source.reservation();
                        let stock_scope = read_current_field_service_stock_scope(
                            c,
                            FieldServiceStockScopeRequest {
                                company_id: config.company.clone(),
                                branch_id: RecordId::new(source.branch_id())?,
                                warehouse_id: part.warehouse_id.clone(),
                                product_id: part.product_id.clone(),
                            },
                        )
                        .await?
                        .ok_or_else(denied)?;
                        let stock =
                            read_field_service_stock(c, &part.warehouse_id, &part.product_id)
                                .await?
                                .ok_or_else(denied)?;
                        let s = stock.data();
                        let reply = FieldServiceReservationReadResponse {
                            context: context(current)?,
                            order: source.order().clone(),
                            receipt: source.receipt().clone(),
                            event: source.event(),
                            reservation: part.clone(),
                            stock: FieldServiceStockSnapshot {
                                warehouse_id: s.warehouse_id.clone(),
                                product_id: s.product_id.clone(),
                                quantity_milli: s.quantity_milli,
                                version: s.version,
                            },
                        };
                        let mut reply_budget = ReplyBudget(0);
                        serde_json::to_writer(&mut reply_budget, &reply).map_err(|_| {
                            BusinessError::new(
                                FailureKind::Unavailable,
                                "FIELD_SERVICE_READ_LIMIT",
                                "The Service source exceeds this bounded read.",
                            )
                        })?;
                        source
                            .require_current(
                                c,
                                current,
                                (2 * 1024 * 1024_usize).saturating_sub(reply_budget.0),
                            )
                            .await?;
                        stock_scope.require_current(c).await?;
                        stock.require_current(c).await?;
                        PgCompanyPurchaseReader::new(
                            config.holding.clone(),
                            config.company.clone(),
                            c,
                            &config.catalogue,
                        )
                        .revalidate(&purchase.witness, clock.now())
                        .await?
                        .require_allowed()?;
                        if clock.now().get() >= current.expires_at() {
                            return Err(denied().into());
                        }
                        Ok(reply)
                    })
                },
            )
            .await
    }
    pub async fn read_agreement_review<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        key: String,
    ) -> Result<FieldServiceAgreementReviewReadResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        if !self.config.agreement_reviews {
            return Err(
                BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.").into(),
            );
        }
        let company = self.config.company.clone();
        let input = move || {
            let mut v = input()?;
            if v.company_id != company {
                return Err(denied());
            }
            v.mutation = false;
            Ok(v)
        };
        let config = self.config.clone();
        let clock = self.clock.clone();
        self.authorizer.execute_with_sources_deferred(
            host, proof, input,
            WorkAuthorizationRequest { permission: "records.read".into(), module: Some("field_service".into()), destination: "service/orders".into(), api: true },
            AgreementReadSources,
            move |c, current| {
                let config = config.clone(); let clock = clock.clone(); let key = key.clone();
                Box::pin(async move {
                    if current.holding_id() != &config.holding || current.selection().workspace().company_id != config.company { return Err(denied().into()); }
                    let source = erpx_server_field_service::public::reservation_read::read_agreement_review_by_key(c,current,&key).await?;
                    let mut caps = vec!["service.orders".into(), "service.agreements".into()];
                    if source.has_asset() { caps.push("service.parts-assets".into()); }
                    let purchase = PgCompanyPurchaseReader::new(config.holding.clone(),config.company.clone(),c,&config.catalogue)
                        .inspect(&PurchaseRequest { capabilities:caps, purpose:CapabilityPurpose::History, mode:PackageEnforcementMode::Enforced },clock.now()).await?;
                    purchase.require_allowed()?;
                    let reply = FieldServiceAgreementReviewReadResponse { context:context(current)?, agreement:source.agreement().clone(),receipt:source.receipt().clone(),event:source.event().clone() };
                    let mut budget=ReplyBudget(0);
                    serde_json::to_writer(&mut budget,&reply).map_err(|_|BusinessError::new(FailureKind::Unavailable,"FIELD_SERVICE_READ_LIMIT","The Service source exceeds this bounded read."))?;
                    source.require_current(c,current).await?;
                    PgCompanyPurchaseReader::new(config.holding.clone(),config.company.clone(),c,&config.catalogue).revalidate(&purchase.witness,clock.now()).await?.require_allowed()?;
                    if clock.now().get()>=current.expires_at(){return Err(denied().into());}
                    Ok(reply)
                })
            }
        ).await
    }
    pub async fn read_agreement_create<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        key: String,
    ) -> Result<FieldServiceAgreementCreateReadResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        if !self.config.agreement_creates {
            return Err(
                BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.").into(),
            );
        }
        let company = self.config.company.clone();
        let input = move || {
            let mut v = input()?;
            if v.company_id != company {
                return Err(denied());
            }
            v.mutation = false;
            Ok(v)
        };
        let config = self.config.clone();
        let clock = self.clock.clone();
        self.authorizer.execute_with_sources_deferred(
            host, proof, input,
            WorkAuthorizationRequest { permission: "records.read".into(), module: Some("field_service".into()), destination: "service/orders".into(), api: true },
            AgreementCreateReadSources,
            move |c, current| {
                let config = config.clone(); let clock = clock.clone(); let key = key.clone();
                Box::pin(async move {
                    if current.holding_id() != &config.holding || current.selection().workspace().company_id != config.company { return Err(denied().into()); }
                    let source = erpx_server_field_service::public::reservation_read::read_agreement_create_by_key(c,current,&key).await?;
                    let mut caps = vec!["service.orders".into(), "service.agreements".into()];
                    if source.has_asset() { caps.push("service.parts-assets".into()); }
                    let purchase = PgCompanyPurchaseReader::new(config.holding.clone(),config.company.clone(),c,&config.catalogue)
                        .inspect(&PurchaseRequest { capabilities:caps, purpose:CapabilityPurpose::History, mode:PackageEnforcementMode::Enforced },clock.now()).await?;
                    purchase.require_allowed()?;
                    let money = erpx_server_finance::public::field_service_agreement_money::read_agreement_money(c,&config.company,&source.agreement().id).await?.ok_or_else(denied)?;
                    let reply = FieldServiceAgreementCreateReadResponse { context:context(current)?, agreement:source.agreement().clone(),receipt:source.receipt().clone(),event:source.event().clone(),money:money.evidence().clone() };
                    let mut budget=ReplyBudget(0);
                    serde_json::to_writer(&mut budget,&reply).map_err(|_|BusinessError::new(FailureKind::Unavailable,"FIELD_SERVICE_READ_LIMIT","The Service source exceeds this bounded read."))?;
                    source.require_current(c,current).await?;
                    money.require_current(c).await?;
                    PgCompanyPurchaseReader::new(config.holding.clone(),config.company.clone(),c,&config.catalogue).revalidate(&purchase.witness,clock.now()).await?.require_allowed()?;
                    if clock.now().get()>=current.expires_at(){return Err(denied().into());}
                    Ok(reply)
                })
            }
        ).await
    }
    pub async fn read_visit<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        key: String,
    ) -> Result<FieldServiceVisitReadResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        if !self.config.visits
            && !self.config.completions
            && !self.config.recoveries
            && !self.config.cancellations
        {
            return Err(
                BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.").into(),
            );
        }
        let company = self.config.company.clone();
        let input = move || {
            let mut v = input()?;
            if v.company_id != company {
                return Err(denied());
            }
            v.mutation = false;
            Ok(v)
        };
        let config = self.config.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                WorkAuthorizationRequest {
                    permission: "records.read".into(),
                    module: Some("field_service".into()),
                    destination: "service/orders".into(),
                    api: true,
                },
                ReadSources { issues: false },
                move |c, current| {
                    let config = config.clone();
                    let clock = clock.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        if current.holding_id() != &config.holding
                            || current.selection().workspace().company_id != config.company
                        {
                            return Err(denied().into());
                        }
                        let source = read_field_service_visit_by_key(c, current, &key).await?;
                        let allowed = match source.event().action.as_str() {
                            "complete" => config.completions,
                            "exception" | "resume" => config.recoveries,
                            "cancel" => config.cancellations,
                            _ => config.visits,
                        };
                        if !allowed {
                            return Err(BusinessError::new(
                                FailureKind::NotFound,
                                "NOT_FOUND",
                                "Not found.",
                            )
                            .into());
                        }
                        let mut caps = vec!["service.orders".into()];
                        match source.event().action.as_str() {
                            "cancel" => {}
                            "schedule" | "dispatch" => caps.push("service.dispatch".into()),
                            _ => caps.push("service.visits".into()),
                        }
                        if source.has_asset() {
                            caps.push("service.parts-assets".into());
                        }
                        let purchase = PgCompanyPurchaseReader::new(
                            config.holding.clone(),
                            config.company.clone(),
                            c,
                            &config.catalogue,
                        )
                        .inspect(
                            &PurchaseRequest {
                                capabilities: caps,
                                purpose: CapabilityPurpose::History,
                                mode: PackageEnforcementMode::Enforced,
                            },
                            clock.now(),
                        )
                        .await?;
                        purchase.require_allowed()?;
                        let reply = FieldServiceVisitReadResponse {
                            context: context(current)?,
                            order: source.order().clone(),
                            receipt: source.receipt().clone(),
                            event: source.event().clone(),
                        };
                        let mut budget = ReplyBudget(0);
                        serde_json::to_writer(&mut budget, &reply).map_err(|_| {
                            BusinessError::new(
                                FailureKind::Unavailable,
                                "FIELD_SERVICE_READ_LIMIT",
                                "The Service source exceeds this bounded read.",
                            )
                        })?;
                        source.require_current(c, current).await?;
                        PgCompanyPurchaseReader::new(
                            config.holding.clone(),
                            config.company.clone(),
                            c,
                            &config.catalogue,
                        )
                        .revalidate(&purchase.witness, clock.now())
                        .await?
                        .require_allowed()?;
                        if clock.now().get() >= current.expires_at() {
                            return Err(denied().into());
                        }
                        Ok(reply)
                    })
                },
            )
            .await
    }
    pub async fn read_issue<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        order: String,
        key: String,
    ) -> Result<FieldServiceIssueReadResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        if !self.config.issues {
            return Err(
                BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.").into(),
            );
        }
        let company = self.config.company.clone();
        let input = move || {
            let mut selection = input()?;
            if selection.company_id != company {
                return Err(denied());
            }
            selection.mutation = false;
            Ok(selection)
        };
        let config = self.config.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                WorkAuthorizationRequest {
                    permission: "records.read".into(),
                    module: Some("field_service".into()),
                    destination: "service/orders".into(),
                    api: true,
                },
                ReadSources { issues: true },
                move |c, current| {
                    let config = config.clone();
                    let clock = clock.clone();
                    let order = order.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        if current.holding_id() != &config.holding
                            || current.selection().workspace().company_id != config.company
                        {
                            return Err(denied().into());
                        }
                        current.require_permission("records.read")?;
                        let purchase = PgCompanyPurchaseReader::new(
                            config.holding.clone(),
                            config.company.clone(),
                            c,
                            &config.catalogue,
                        )
                        .inspect(
                            &PurchaseRequest {
                                capabilities: vec![
                                    "service.orders".into(),
                                    "service.parts-assets".into(),
                                ],
                                purpose: CapabilityPurpose::History,
                                mode: PackageEnforcementMode::Enforced,
                            },
                            clock.now(),
                        )
                        .await?;
                        purchase.require_allowed()?;
                        let source = read_field_service_issue(c, current, &order, &key).await?;
                        let part = source.reservation().reservation();
                        let consumption = erpx_server_inventory::public::field_service_consumption_finance::read_field_service_consumption_finance(
                            c, &config.holding, &config.company, &source.event().consumption_id
                        ).await?.ok_or_else(denied)?;
                        let facts = consumption.facts();
                        let event = source.event();
                        if facts.order_id != order || facts.source_version != event.version || facts.creator_user_id != event.actor_id
                            || facts.journal_id != event.journal_id || facts.quantity_milli != event.quantity_milli
                            || facts.value_minor != event.cost_minor || facts.warehouse_id != part.warehouse_id
                            || facts.product_id != part.product_id || facts.branch_id != source.reservation().branch_id()
                        { return Err(denied().into()); }
                        let stock_scope = read_current_field_service_stock_scope(
                            c,
                            FieldServiceStockScopeRequest {
                                company_id: config.company.clone(),
                                branch_id: RecordId::new(source.reservation().branch_id())?,
                                warehouse_id: part.warehouse_id.clone(),
                                product_id: part.product_id.clone(),
                            },
                        )
                        .await?
                        .ok_or_else(denied)?;
                        let stock =
                            read_field_service_stock(c, &part.warehouse_id, &part.product_id)
                                .await?
                                .ok_or_else(denied)?;
                        let s = stock.data();
                        let reply = FieldServiceIssueReadResponse {
                            context: context(current)?,
                            order: source.reservation().order().clone(),
                            receipt: source.receipt().clone(),
                            event: source.event().clone(),
                            reservation: part.clone(),
                            stock: FieldServiceStockSnapshot {
                                warehouse_id: s.warehouse_id.clone(),
                                product_id: s.product_id.clone(),
                                quantity_milli: s.quantity_milli,
                                version: s.version,
                            },
                        };
                        let mut reply_budget = ReplyBudget(0);
                        serde_json::to_writer(&mut reply_budget, &reply).map_err(|_| {
                            BusinessError::new(
                                FailureKind::Unavailable,
                                "FIELD_SERVICE_READ_LIMIT",
                                "The Service source exceeds this bounded read.",
                            )
                        })?;
                        source
                            .require_current(
                                c,
                                current,
                                (2 * 1024 * 1024_usize).saturating_sub(reply_budget.0),
                            )
                            .await?;
                        consumption.require_current(c).await?;
                        stock_scope.require_current(c).await?;
                        stock.require_current(c).await?;
                        PgCompanyPurchaseReader::new(
                            config.holding.clone(),
                            config.company.clone(),
                            c,
                            &config.catalogue,
                        )
                        .revalidate(&purchase.witness, clock.now())
                        .await?
                        .require_allowed()?;
                        if clock.now().get() >= current.expires_at() {
                            return Err(denied().into());
                        }
                        Ok(reply)
                    })
                },
            )
            .await
    }
    pub async fn read_request<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        key: String,
    ) -> Result<FieldServiceRequestReadResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        use erpx_server_assets::public::field_service_reference::read_current_field_service_asset;
        use erpx_server_field_service::public::{
            request_agreement::read_service_request_agreement_history,
            request_order::read_service_request_order,
            reservation_read::{
                FieldServiceRequestEvent, FieldServiceRequestOrderRead, FieldServiceRequestResult,
                FieldServiceRequestTicketRead,
            },
        };
        use erpx_server_finance::public::field_service_customer::read_current_field_service_customer;
        use erpx_server_support::public::field_service_request::read_service_intake;
        use erpx_server_workflow::public::record_origin::{
            RawRecordOriginRequest, read_raw_record_origin_result_with_limit,
            verify_raw_record_origin,
        };
        if !self.config.requests {
            return Err(
                BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.").into(),
            );
        }
        erpx_server_shared::public::CommandKey::new(key.clone())?;
        let company = self.config.company.clone();
        let input = move || {
            let mut selection = input()?;
            if selection.company_id != company {
                return Err(denied());
            }
            selection.mutation = false;
            Ok(selection)
        };
        let config = self.config.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                WorkAuthorizationRequest {
                    permission: "records.read".into(),
                    module: Some("field_service".into()),
                    destination: "service/orders".into(),
                    api: true,
                },
                crate::field_service_parts::FieldServicePartsSourcePreparation,
                move |c, current| {
                    let config = config.clone();
                    let clock = clock.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        if current.holding_id() != &config.holding
                            || current.selection().workspace().company_id != config.company
                        {
                            return Err(denied().into());
                        }
                        current.require_permission("records.read")?;
                        let order = read_service_request_order(c, current, &key)
                            .await?
                            .ok_or_else(denied)?;
                        let ticket = read_service_intake(c, &config.company, &key)
                            .await?
                            .ok_or_else(denied)?;
                        let o = order.facts();
                        let t = ticket.facts();
                        if o.order_id != t.order_id
                            || o.ticket_id != t.ticket_id
                            || o.agreement_id != t.agreement_id
                            || o.agreement_version != t.agreement_version
                            || o.actor_id != t.actor_id
                            || o.event_status != if o.scheduled_at.is_some() { "scheduled" } else { "open" }
                            || o.initial_at != t.initial_at || o.issue != t.initial_issue || o.technician_id != t.initial_owner
                        {
                            return Err(denied().into());
                        }
                        let agreement = read_service_request_agreement_history(
                            c,
                            current,
                            &o.agreement_id,
                        )
                        .await?
                        .ok_or_else(denied)?;
                        let a = agreement.facts();
                        if a.branch_id.as_str()!=o.branch_id || a.department_id.as_ref().map(|v|v.as_str())!=o.department_id.as_deref() || a.customer_id.as_str() != t.customer_id || a.branch_id.as_str() != t.branch_id || a.department_id.as_ref().map(|v|v.as_str()) != t.department_id.as_deref() {
                            return Err(denied().into());
                        }
                        let customer =
                            read_current_field_service_customer(c, &config.company, &a.customer_id)
                                .await?
                                .ok_or_else(denied)?;
                        let db_now: i64=sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint").fetch_one(&mut *c).await?;
                        let technician = erpx_server_identity_access::public::field_service_assignment::inspect_field_service_assignment(c, erpx_server_identity_access::public::field_service_assignment::FieldServiceAssignmentRequest { company_id:config.company.clone(),user_id:o.current_technician_id.clone(),branch_id:a.branch_id.clone(),department_id:a.department_id.clone(),permission:erpx_server_identity_access::public::field_service_assignment::FieldServiceAssignmentPermission::Write },TimestampMillis::new(db_now)).await?;
                        let person = erpx_server_people_payroll::public::field_service_worker::inspect_field_service_people_reference(c,&config.company,&o.current_technician_id).await?;
                        let asset = if let Some(id) = &a.asset_id {
                            let v = read_current_field_service_asset(
                                c,
                                &config.holding,
                                &config.company,
                                id,
                            )
                            .await?
                            .ok_or_else(denied)?;
                            if v.facts().branch_id != a.branch_id {
                                return Err(denied().into());
                            }
                            Some(v)
                        } else {
                            None
                        };
                        let mut caps =
                            vec!["service.orders".into(), "support.source-ticket".into()];
                        if asset.is_some() {
                            caps.push("service.parts-assets".into());
                        }
                        let mut purchases = PgCompanyPurchaseReader::new(
                            config.holding.clone(),
                            config.company.clone(),
                            c,
                            &config.catalogue,
                        );
                        let purchase = purchases
                            .inspect(
                                &PurchaseRequest {
                                    capabilities: caps,
                                    purpose: CapabilityPurpose::History,
                                    mode: PackageEnforcementMode::Enforced,
                                },
                                clock.now(),
                            )
                            .await?
                            .require_allowed()?
                            .clone();
                        let (origin, result) = read_raw_record_origin_result_with_limit(
                            c,
                            RawRecordOriginRequest {
                                command_key: key.clone(),
                                operation: "field.order.request".into(),
                                record_id: o.order_id.clone(),
                                creator_user_id: o.actor_id.clone(),
                                company_id: config.company.clone(),
                            },
                            1024 * 1024,
                        )
                        .await?
                        .ok_or_else(denied)?;
                        let result_id = result
                            .property("id")?
                            .and_then(|v| v.string_utf8_storage().ok().flatten())
                            .ok_or_else(denied)?;
                        let result_ticket = result
                            .property("ticketId")?
                            .and_then(|v| v.string_utf8_storage().ok().flatten())
                            .ok_or_else(denied)?;
                        let status = result
                            .property("status")?
                            .and_then(|v| v.string_utf8_storage().ok().flatten())
                            .ok_or_else(denied)?;
                        let version = result
                            .property("version")?
                            .and_then(|v| v.as_str().parse::<i64>().ok())
                            .ok_or_else(denied)?;
                        if result_id != o.order_id
                            || result_ticket != o.ticket_id
                            || status != o.event_status
                            || version != 1
                        {
                            return Err(denied().into());
                        }
                        let reply = FieldServiceRequestReadResponse {
                            context: context(current)?,
                            receipt: FieldServiceRequestResult {
                                id: result_id,
                                ticket_id: result_ticket,
                                status,
                                version,
                            },
                            order: FieldServiceRequestOrderRead {
                                id: o.order_id.clone(),
                                ticket_id: o.ticket_id.clone(),
                                agreement_id: o.agreement_id.clone(),
                                agreement_version: o.agreement_version,
                                customer_id: t.customer_id.clone(),
                                status: o.status.clone(),
                                version: o.version,
                            },
                            ticket: FieldServiceRequestTicketRead {
                                id: t.ticket_id.clone(),
                                status: t.status.clone(),
                                version: t.version,
                            },
                            event: FieldServiceRequestEvent {
                                actor_id: o.actor_id.clone(),
                                status: o.event_status.clone(),
                                version: 1,
                                issue: o.issue.clone(),
                                site: o.site.clone(),
                                technician_id: o.technician_id.clone(),
                                scheduled_at: o.scheduled_at,
                            },
                        };
                        let mut budget = ReplyBudget(0);
                        serde_json::to_writer(&mut budget, &reply).map_err(|_| {
                            BusinessError::new(
                                FailureKind::Unavailable,
                                "FIELD_SERVICE_READ_LIMIT",
                                "The Service source exceeds this bounded read.",
                            )
                        })?;
                        order.require_current(c, current, &key).await?;
                        ticket.require_current(c, &config.company, &key).await?;
                        ticket.require_initial_schedule(c,&key,o.scheduled_at,o.initial_at).await?;
                        agreement.require_current(c, current).await?;
                        customer.require_current(c).await?;
                        let db_now:i64=sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint").fetch_one(&mut *c).await?;
                        technician.require_current(c,TimestampMillis::new(db_now)).await?; person.require_current(c).await?;
                        if let Some(v) = asset {
                            v.require_current(c).await?;
                        }
                        verify_raw_record_origin(c, &origin).await?;
                        PgCompanyPurchaseReader::new(
                            config.holding.clone(),
                            config.company.clone(),
                            c,
                            &config.catalogue,
                        )
                        .revalidate(&purchase, clock.now())
                        .await?
                        .require_allowed()?;
                        if clock.now().get() >= current.expires_at() {
                            return Err(denied().into());
                        }
                        Ok(reply)
                    })
                },
            )
            .await
    }
    pub async fn read_created_order<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        key: String,
    ) -> Result<FieldServiceOrderCreateReadResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        use erpx_server_assets::public::field_service_reference::read_current_field_service_physical_asset;
        use erpx_server_field_service::public::{
            request_agreement::read_service_request_agreement_history,
            request_order::read_service_request_order,
            reservation_read::{
                FieldServiceCreatedOrderRead, FieldServiceRequestEvent,
                FieldServiceRequestTicketRead,
            },
        };
        use erpx_server_finance::public::field_service_customer::read_current_field_service_customer;
        use erpx_server_support::public::field_service_existing_ticket::read_existing_service_ticket;
        use erpx_server_workflow::public::record_origin::{
            RawRecordOriginRequest, read_raw_record_origin_result_with_limit,
            verify_raw_record_origin,
        };
        if !self.config.order_creates {
            return Err(
                BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.").into(),
            );
        }
        erpx_server_shared::public::CommandKey::new(key.clone())?;
        let company = self.config.company.clone();
        let input = move || {
            let mut v = input()?;
            if v.company_id != company {
                return Err(denied());
            }
            v.mutation = false;
            Ok(v)
        };
        let config = self.config.clone();
        let clock = self.clock.clone();
        self.authorizer.execute_with_sources_deferred(host,proof,input,WorkAuthorizationRequest{permission:"records.read".into(),module:Some("field_service".into()),destination:"service/orders".into(),api:true},crate::field_service_parts::FieldServicePartsSourcePreparation,move|c,current|{let config=config.clone();let clock=clock.clone();let key=key.clone();Box::pin(async move{
   if current.holding_id()!=&config.holding||current.selection().workspace().company_id!=config.company{return Err(denied().into());}current.require_permission("records.read")?;
   let order=read_service_request_order(c,current,&key).await?.ok_or_else(denied)?;let o=order.facts();
   let ticket=read_existing_service_ticket(c,&config.company,&o.ticket_id).await?.ok_or_else(denied)?;let t=ticket.facts();
   let agreement=read_service_request_agreement_history(c,current,&o.agreement_id).await?.ok_or_else(denied)?;let a=agreement.facts();
   if o.branch_id!=t.branch_id.as_str()||o.department_id.as_deref()!=t.department_id.as_ref().map(|v|v.as_str())||a.branch_id!=t.branch_id||a.department_id!=t.department_id||a.customer_id!=t.customer_id||t.version<o.ticket_version||a.asset_id.as_ref().is_some_and(|v|Some(v.as_str())!=o.asset_id.as_deref())||o.event_status!=if o.scheduled_at.is_some(){"scheduled"}else{"open"}{return Err(denied().into());}
   let customer=read_current_field_service_customer(c,&config.company,&a.customer_id).await?.ok_or_else(denied)?;
   let asset=if let Some(id)=o.asset_id.as_ref().filter(|v|!v.is_empty()){let v=read_current_field_service_physical_asset(c,&config.holding,&config.company,id).await?.ok_or_else(denied)?;if v.facts().branch_id!=a.branch_id{return Err(denied().into());}Some(v)}else{None};
   let db_now:i64=sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint").fetch_one(&mut *c).await?;
   let technician=erpx_server_identity_access::public::field_service_assignment::inspect_field_service_assignment(c,erpx_server_identity_access::public::field_service_assignment::FieldServiceAssignmentRequest{company_id:config.company.clone(),user_id:o.current_technician_id.clone(),branch_id:a.branch_id.clone(),department_id:a.department_id.clone(),permission:erpx_server_identity_access::public::field_service_assignment::FieldServiceAssignmentPermission::Write},TimestampMillis::new(db_now)).await?;
   let person=erpx_server_people_payroll::public::field_service_worker::inspect_field_service_people_reference(c,&config.company,&o.current_technician_id).await?;
   let mut caps=vec!["service.orders".into(),"support.source-ticket".into()];if asset.is_some()||a.asset_id.is_some(){caps.push("service.parts-assets".into());}let purchase=PgCompanyPurchaseReader::new(config.holding.clone(),config.company.clone(),c,&config.catalogue).inspect(&PurchaseRequest{capabilities:caps,purpose:CapabilityPurpose::History,mode:PackageEnforcementMode::Enforced},clock.now()).await?.require_allowed()?.clone();
   let (origin,result)=read_raw_record_origin_result_with_limit(c,RawRecordOriginRequest{command_key:key.clone(),operation:"field.order.create".into(),record_id:o.order_id.clone(),creator_user_id:o.actor_id.clone(),company_id:config.company.clone()},1024*1024).await?.ok_or_else(denied)?;
   let id=result.property("id")?.and_then(|v|v.string_utf8_storage().ok().flatten()).ok_or_else(denied)?;let status=result.property("status")?.and_then(|v|v.string_utf8_storage().ok().flatten()).ok_or_else(denied)?;let version=result.property("version")?.and_then(|v|v.as_str().parse::<i64>().ok()).ok_or_else(denied)?;if id!=o.order_id||status!=o.event_status||version!=1{return Err(denied().into());}
   let reply=FieldServiceOrderCreateReadResponse{context:context(current)?,receipt:FieldServiceReserveResult{id,status,version},order:FieldServiceCreatedOrderRead{id:o.order_id.clone(),ticket_id:o.ticket_id.clone(),ticket_version:o.ticket_version,agreement_id:o.agreement_id.clone(),agreement_version:o.agreement_version,customer_id:t.customer_id.as_str().into(),asset_id:o.asset_id.clone(),status:o.status.clone(),version:o.version},ticket:FieldServiceRequestTicketRead{id:t.id.clone(),status:t.status.clone(),version:t.version},event:FieldServiceRequestEvent{actor_id:o.actor_id.clone(),status:o.event_status.clone(),version:1,issue:o.issue.clone(),site:o.site.clone(),technician_id:o.technician_id.clone(),scheduled_at:o.scheduled_at}};
   let mut budget=ReplyBudget(0);serde_json::to_writer(&mut budget,&reply).map_err(|_|BusinessError::new(FailureKind::Unavailable,"FIELD_SERVICE_READ_LIMIT","The Service source exceeds this bounded read."))?;
   order.require_current(c,current,&key).await?;ticket.require_current(c).await?;agreement.require_current(c,current).await?;customer.require_current(c).await?;if let Some(v)=asset{v.require_current(c).await?;}let db_now:i64=sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint").fetch_one(&mut *c).await?;technician.require_current(c,TimestampMillis::new(db_now)).await?;person.require_current(c).await?;verify_raw_record_origin(c,&origin).await?;
   PgCompanyPurchaseReader::new(config.holding.clone(),config.company.clone(),c,&config.catalogue).revalidate(&purchase,clock.now()).await?.require_allowed()?;if clock.now().get()>=current.expires_at(){return Err(denied().into());}Ok(reply)
  })}).await
    }
}
