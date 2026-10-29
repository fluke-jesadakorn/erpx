//! Development read assembly. Rendered context is output data from the SAME
//! retained authorization unit, never a deserialized actor or a command grant.
use crate::finance_journal_review::FinanceJournalEnvironment;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, lock_commercial_authority_sources,
};
use erpx_server_finance::public::{FinanceActor, journal_detail::*, journal_workspace::*};
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_people_payroll::public::finance_journal_names::{
    lock_finance_journal_name_sources, read_finance_journal_names,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, FailureKind, HoldingId, RecordId, TimestampMillis, UserId,
};
use serde::Serialize;
use sqlx::PgConnection;
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;
#[derive(Clone)]
pub struct FinanceJournalReadConfiguration {
    holding: HoldingId,
    company: CompanyId,
    purchases: Arc<CommercialCatalogue>,
    field_service_parts: bool,
    field_service_invoices: bool,
}
impl FinanceJournalReadConfiguration {
    pub fn new(
        environment: FinanceJournalEnvironment,
        holding: HoldingId,
        company: CompanyId,
        purchases: Arc<CommercialCatalogue>,
    ) -> Result<Self, BusinessError> {
        if matches!(environment, FinanceJournalEnvironment::Production) {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "FINANCE_PRODUCTION_COMPOSITION_UNAVAILABLE",
                "Finance release and Company acceptance ports are incomplete.",
            ));
        }
        Ok(Self {
            holding,
            company,
            purchases,
            field_service_parts: false,
            field_service_invoices: false,
        })
    }
    pub fn with_field_service_invoices(mut self) -> Self {
        self.field_service_invoices = true;
        self
    }
    pub fn with_field_service_parts(mut self) -> Self {
        self.field_service_parts = true;
        self
    }
    pub fn company_id(&self) -> &CompanyId {
        &self.company
    }
}
impl HoldingWorkSourcePreparation for FinanceJournalReadConfiguration {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_journal_detail_sources(c).await?;
            lock_finance_journal_name_sources(c).await
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceRenderedWorkContext {
    pub user_id: String,
    pub holding_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub branch_slug: Option<String>,
    pub department_slug: Option<String>,
    pub assignment_id: String,
    pub assignment_version: FinanceReadInteger,
    pub revision: FinanceReadInteger,
    pub role: String,
    pub assignment_scope_kind: String,
    pub modules: Vec<String>,
    pub permissions: Vec<String>,
    pub expires_at: FinanceReadInteger,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceJournalReadResponse {
    pub data: FinanceJournalDetail,
    pub context: FinanceRenderedWorkContext,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceJournalWorkspaceReadResponse {
    pub data: FinanceJournalRecordWorkspace,
    pub context: FinanceRenderedWorkContext,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceServiceInvoiceWorkspaceResponse {
    pub data:
        erpx_server_finance::public::field_service_invoice_read::FinanceServiceInvoiceWorkspace,
    pub context: FinanceRenderedWorkContext,
    #[serde(rename = "sourceOnly")]
    pub source_only: bool,
}
#[derive(Clone)]
struct ServiceInvoiceReadSources;
impl HoldingWorkSourcePreparation for ServiceInvoiceReadSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            erpx_server_finance::public::field_service_invoice_read::lock_service_invoice_sources(
                c,
            )
            .await?;
            lock_commercial_authority_sources(c).await?;
            erpx_server_workflow::public::record_origin::lock_record_origin_sources(c).await?;
            erpx_server_field_service::public::finance_lineage::lock_field_service_invoice_finance_sources(c).await
        })
    }
}
#[derive(Clone, Copy)]
enum ReadKind {
    Detail,
    Workspace,
}
#[derive(Clone)]
struct ReadPreparation {
    kind: ReadKind,
    field_service_parts: bool,
}
impl HoldingWorkSourcePreparation for ReadPreparation {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            match self.kind {
                ReadKind::Detail => lock_journal_detail_sources(c).await?,
                ReadKind::Workspace => lock_journal_workspace_sources(c).await?,
            }
            if self.field_service_parts {
                erpx_server_finance::public::journal_review::lock_journal_review_sources(c).await?;
                erpx_server_inventory::public::field_service_consumption_finance::lock_consumption_finance_sources(c).await?;
                erpx_server_field_service::public::finance_lineage::lock_field_service_parts_finance_sources(c).await?;
            }
            lock_finance_journal_name_sources(c).await
        })
    }
}
enum ReadSource {
    Detail(Box<CurrentJournalDetail>),
    Workspace(Box<CurrentJournalRecordWorkspace>),
    FieldDetail(
        Box<CurrentFieldServiceJournalDetail>,
        Box<crate::finance_journal_review::field_service_parts::FieldServicePartsSource>,
    ),
    FieldWorkspace(
        Box<CurrentFieldServiceJournalWorkspace>,
        Box<crate::finance_journal_review::field_service_parts::FieldServicePartsSource>,
    ),
}
impl ReadSource {
    fn journal(&self) -> &FinanceLedgerRow {
        match self {
            Self::Detail(s) => &s.detail().journal,
            Self::Workspace(s) => &s.data().journal,
            Self::FieldDetail(s, _) => &s.detail().journal,
            Self::FieldWorkspace(s, _) => &s.data().journal,
        }
    }
    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        match self {
            Self::Detail(s) => s.require_current(c).await,
            Self::Workspace(s) => s.require_current(c).await,
            Self::FieldDetail(s, lineage) => {
                s.require_current(c).await?;
                lineage.require_current(c).await
            }
            Self::FieldWorkspace(s, lineage) => {
                s.require_current(c).await?;
                lineage.require_current(c).await
            }
        }
    }
    fn response(
        &self,
        context: FinanceRenderedWorkContext,
        creator: &str,
        poster: &str,
    ) -> ReadResponse {
        let names = |row: &mut FinanceLedgerRow| {
            row.insert(
                "created_by_name".into(),
                FinanceLedgerValue::Text(creator.into()),
            );
            row.insert(
                "posted_by_name".into(),
                FinanceLedgerValue::Text(poster.into()),
            );
        };
        match self {
            Self::Detail(s) => {
                let mut data = s.detail().clone();
                names(&mut data.journal);
                ReadResponse::Detail(FinanceJournalReadResponse { data, context })
            }
            Self::FieldDetail(s, _) => {
                let mut data = s.detail().clone();
                names(&mut data.journal);
                ReadResponse::Detail(FinanceJournalReadResponse { data, context })
            }
            Self::FieldWorkspace(s, _) => {
                let mut data = s.data().clone();
                names(&mut data.journal);
                ReadResponse::Workspace(FinanceJournalWorkspaceReadResponse { data, context })
            }
            Self::Workspace(s) => {
                let mut data = s.data().clone();
                names(&mut data.journal);
                ReadResponse::Workspace(FinanceJournalWorkspaceReadResponse { data, context })
            }
        }
    }
}
#[derive(Serialize)]
#[serde(untagged)]
enum ReadResponse {
    Detail(FinanceJournalReadResponse),
    Workspace(FinanceJournalWorkspaceReadResponse),
}
fn rendered(current: &AuthorizedWorkContext) -> Result<FinanceRenderedWorkContext, BusinessError> {
    let selection = current.selection();
    let workspace = selection.workspace();
    let grant = selection.assignment();
    Ok(FinanceRenderedWorkContext {
        user_id: current.user_id().as_str().into(),
        holding_id: current.holding_id().as_str().into(),
        company_id: workspace.company_id.as_str().into(),
        branch_id: workspace.branch_id.as_ref().map(|v| v.as_str().into()),
        department_id: workspace.department_id.as_ref().map(|v| v.as_str().into()),
        branch_slug: workspace.branch_slug.clone(),
        department_slug: workspace.department_slug.clone(),
        assignment_id: grant.id.as_str().into(),
        assignment_version: FinanceReadInteger::new(grant.version)?,
        revision: FinanceReadInteger::new(selection.revision())?,
        role: grant.role.clone(),
        assignment_scope_kind: match grant.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .into(),
        modules: current.modules().to_vec(),
        permissions: current.command_iam_metadata().permissions.clone(),
        expires_at: FinanceReadInteger::new(current.expires_at())?,
    })
}
struct ReplyBudget(usize);
impl std::io::Write for ReplyBudget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .filter(|n| *n <= 2 * 1024 * 1024)
            .ok_or_else(|| std::io::Error::other("bounded Finance reply"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn admit_reply(reply: &ReadResponse) -> Result<(), BusinessError> {
    serde_json::to_writer(ReplyBudget(0), reply).map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "JOURNAL_DETAIL_RESPONSE_LIMIT",
            "The journal exceeds this bounded read.",
        )
    })
}
fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "JOURNAL_NOT_FOUND",
        "Journal not found.",
    )
}
pub struct FinanceJournalDetailService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: FinanceJournalReadConfiguration,
    clock: Arc<C>,
}
impl<C: Clock + Send + Sync + 'static> FinanceJournalDetailService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: FinanceJournalReadConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
        }
    }
    pub fn company_id(&self) -> &CompanyId {
        &self.configuration.company
    }
    pub async fn read_service_invoice<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        id: String,
    ) -> Result<FinanceServiceInvoiceWorkspaceResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        if !self.configuration.field_service_invoices {
            return Err(scope().into());
        }
        let company = self.configuration.company.clone();
        let input = move || {
            let mut v = input()?;
            if v.company_id != company {
                return Err(scope());
            }
            v.mutation = false;
            Ok(v)
        };
        let config = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer.execute_with_sources_deferred(host,proof,input,WorkAuthorizationRequest{permission:"finance.read".into(),module:Some("finance".into()),destination:"finance/invoices".into(),api:true},ServiceInvoiceReadSources,move |c,current|{
            let config=config.clone();let clock=clock.clone();let id=id.clone();Box::pin(async move {
                if current.holding_id()!=&config.holding || current.selection().workspace().company_id!=config.company{return Err(scope().into())}
                current.require_permission("finance.read")?;current.require_permission("module.finance.read")?;
                let actor=FinanceActor{holding_id:config.holding.clone(),company_id:config.company.clone(),user_id:current.user_id().clone(),assignment_id:current.selection().assignment().id.clone()};
                let finance=erpx_server_finance::public::field_service_invoice_read::read_service_invoice_workspace(c,&actor,&id).await?;
                let src=&finance.data().source;
                let lineage=erpx_server_field_service::public::finance_lineage::read_field_service_invoice_finance_lineage(c,erpx_server_field_service::public::finance_lineage::FieldServiceInvoiceFinanceRequest{company_id:config.company.clone(),order_id:src.order_id.clone(),release_id:src.release_id.clone(),key:src.command_key.clone(),actor_id:src.actor_id.clone(),currency:src.currency.clone(),amount_units:src.amount_units.clone(),customer_id:finance.data().invoice.get("party_id").and_then(|v|v.as_str()).ok_or_else(scope)?.into(),pricing_date:finance.data().invoice.get("invoice_date").and_then(|v|v.as_str()).ok_or_else(scope)?.into()}).await?.ok_or_else(scope)?;
                let mut reader=PgCompanyPurchaseReader::new(config.holding.clone(),config.company.clone(),c,&config.purchases);
                let req=|cap:&str|PurchaseRequest{capabilities:vec![cap.into()],purpose:CapabilityPurpose::History,mode:PackageEnforcementMode::Enforced};
                let base=reader.inspect(&req("finance.accounting"),clock.now()).await?;
                let source_only=base.require_allowed().is_err();let base_witness=base.witness.clone();let selected=if !source_only {base}else{reader.inspect(&req("finance.service-invoice"),clock.now()).await?};selected.require_allowed()?;
                let reply=FinanceServiceInvoiceWorkspaceResponse{data:finance.data().clone(),context:rendered(current)?,source_only};
                serde_json::to_writer(ReplyBudget(0),&reply).map_err(|_|BusinessError::new(FailureKind::Unavailable,"FINANCE_INVOICE_READ_LIMIT","The invoice workspace exceeds this bounded read."))?;
                finance.require_current(c).await?;lineage.require_current(c).await?;
                let base_current=PgCompanyPurchaseReader::new(config.holding.clone(),config.company.clone(),c,&config.purchases).revalidate(&base_witness,clock.now()).await?;if base_current.require_allowed().is_err()!=source_only{return Err(scope().into())}
                PgCompanyPurchaseReader::new(config.holding.clone(),config.company.clone(),c,&config.purchases).revalidate(&selected.witness,clock.now()).await?.require_allowed()?;
                if clock.now().get()>=current.expires_at(){return Err(scope().into())}Ok(reply)
            })
        }).await
    }
    pub async fn read<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        id: RecordId,
    ) -> Result<FinanceJournalReadResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        match self
            .read_kind(host, proof, input, id, ReadKind::Detail)
            .await?
        {
            ReadResponse::Detail(reply) => Ok(reply),
            _ => Err(scope().into()),
        }
    }
    pub async fn read_workspace<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        id: RecordId,
    ) -> Result<FinanceJournalWorkspaceReadResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        match self
            .read_kind(host, proof, input, id, ReadKind::Workspace)
            .await?
        {
            ReadResponse::Workspace(reply) => Ok(reply),
            _ => Err(scope().into()),
        }
    }
    async fn read_kind<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        id: RecordId,
        kind: ReadKind,
    ) -> Result<ReadResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = false;
            Ok(request)
        };
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        let preparation = ReadPreparation {
            kind,
            field_service_parts: configuration.field_service_parts,
        };
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                WorkAuthorizationRequest {
                    permission: "finance.read".into(),
                    module: Some("finance".into()),
                    destination: "finance/journals".into(),
                    api: true,
                },
                preparation,
                move |c, current| {
                    let configuration = configuration.clone();
                    let clock = clock.clone();
                    let id = id.clone();
                    Box::pin(async move {
                        if current.holding_id() != &configuration.holding
                            || current.selection().workspace().company_id != configuration.company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("finance.read")?;
                        current.require_permission("module.finance.read")?;
                        let inspection = PgCompanyPurchaseReader::new(
                            configuration.holding.clone(),
                            configuration.company.clone(),
                            c,
                            &configuration.purchases,
                        )
                        .inspect(
                            &PurchaseRequest {
                                capabilities: vec!["finance.accounting".into()],
                                purpose: CapabilityPurpose::History,
                                mode: PackageEnforcementMode::Enforced,
                            },
                            clock.now(),
                        )
                        .await?;
                        inspection.require_allowed()?;
                        let purchase = inspection.witness;
                        let actor = FinanceActor {
                            holding_id: current.holding_id().clone(),
                            company_id: configuration.company.clone(),
                            user_id: current.user_id().clone(),
                            assignment_id: current.selection().assignment().id.clone(),
                        };
                        let target = if configuration.field_service_parts {
                            use erpx_server_finance::public::journal_review::{inspect_field_service_parts_journal_target, JournalReviewCommand, JournalReviewInput};
                            let command = JournalReviewCommand { key: erpx_server_shared::public::CommandKey::new("read-only-source")?, journal_id: id.clone(), input: JournalReviewInput::Post { expected_version: 1 } };
                            inspect_field_service_parts_journal_target(c, &actor, &command).await?
                        } else { None };
                        let source = if let Some(target) = target {
                            match kind {
                                ReadKind::Detail => {
                                    let detail = read_field_service_journal_detail(c, &actor, &id, &target).await?;
                                    let lineage = crate::finance_journal_review::field_service_parts::FieldServicePartsSource::retain(c, target).await?;
                                    ReadSource::FieldDetail(Box::new(detail), Box::new(lineage))
                                }
                                ReadKind::Workspace => {
                                    let workspace = read_field_service_journal_workspace(c, &actor, &id, &target).await?;
                                    let lineage = crate::finance_journal_review::field_service_parts::FieldServicePartsSource::retain(c, target).await?;
                                    ReadSource::FieldWorkspace(Box::new(workspace), Box::new(lineage))
                                }
                            }
                        } else {
                            match kind {
                                ReadKind::Detail => ReadSource::Detail(Box::new(read_journal_detail(c, &actor, &id).await?)),
                                ReadKind::Workspace => ReadSource::Workspace(Box::new(read_journal_record_workspace(c, &actor, &id).await?)),
                            }
                        };
                        let creator = match source.journal().get("created_by") {
                            Some(FinanceLedgerValue::Text(v)) => UserId::new(v)?,
                            _ => return Err(scope().into()),
                        };
                        let poster = match source.journal().get("posted_by") {
                            Some(FinanceLedgerValue::Text(v)) => Some(UserId::new(v)?),
                            Some(FinanceLedgerValue::Null) => None,
                            _ => return Err(scope().into()),
                        };
                        let mut users = vec![creator.clone()];
                        if let Some(user) = &poster {
                            users.push(user.clone());
                        }
                        let names =
                            read_finance_journal_names(c, &configuration.company, &users).await?;
                        let response = source.response(
                            rendered(current)?,
                            names.name(&creator)?,
                            match &poster {
                                Some(p) => names.name(p)?,
                                None => "Unlinked account",
                            },
                        );
                        admit_reply(&response)?;
                        source.require_current(c).await?;
                        names.require_current(c).await?;
                        PgCompanyPurchaseReader::new(
                            configuration.holding.clone(),
                            configuration.company.clone(),
                            c,
                            &configuration.purchases,
                        )
                        .revalidate(&purchase, TimestampMillis::new(clock.now().get()))
                        .await?
                        .require_allowed()?;
                        if clock.now().get() >= current.expires_at() {
                            return Err(scope().into());
                        }
                        Ok(response)
                    })
                },
            )
            .await
    }
}
