//! Native Marketing campaign/attribution assembly. Current Account/Holding
//! authority remains with the selected coordinator; owner ports supply the
//! campaign ledger, attribution summary and package entitlement facts.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness, current_package_catalogue, lock_commercial_authority_sources,
};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{ExpectedWorkSelection, HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_marketing::public::campaign_attribution::{
    MarketingCampaignEntitlement, MarketingCampaignEntitlementPort, MarketingCampaignFuture,
};
use erpx_server_marketing::public::campaign_attribution_http::{
    MarketingCampaignHttpBackend, MarketingCampaignHttpFuture, MarketingCampaignHttpReadRequest,
    MarketingCampaignHttpResponse,
};
use erpx_server_marketing::public::lead_candidate::{
    MarketingAssetCreateInput, MarketingAssetReviewInput, MarketingCampaignCreateInput,
    MarketingCommandReceipt, MarketingLeadActionInput, MarketingLeadActor,
    MarketingLeadCandidateInput, PgMarketingLead, asset_profiles,
    create_asset as create_marketing_asset, create_campaign as create_marketing_campaign,
    create_candidate as create_marketing_candidate, lead_detail,
    record_engagement as record_marketing_engagement, review_asset as review_marketing_asset,
};
use erpx_server_marketing::public::lead_candidate_command_http::{
    MarketingAssetReviewHttpRequest, MarketingCommandHttpRequest, MarketingLeadCommandHttpBackend,
    MarketingLeadCommandHttpFuture, MarketingLeadCommandHttpRequest,
};
use erpx_server_marketing::public::lead_candidate_http::{
    MarketingLeadHttpBackend, MarketingLeadHttpFuture, MarketingLeadHttpReadRequest,
    MarketingLeadHttpResponse,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId,
    TimestampMillis,
};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "MARKETING_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MARKETING_UNAVAILABLE",
        "The Marketing workspace is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct MarketingSources;

impl HoldingWorkSourcePreparation for MarketingSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c)
                .await
                .map_err(business)?;
            Ok(())
        })
    }
}

struct MarketingEntitlementImpl;

impl MarketingCampaignEntitlementPort for MarketingEntitlementImpl {
    type Error = BusinessError;

    fn entitlement<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> MarketingCampaignFuture<'a, MarketingCampaignEntitlement, Self::Error> {
        // The selected composition has not yet bound trusted Commercial
        // enforcement and purchase facts. Do not treat their absence as the
        // disabled mode: that could expose data while enforcement is active.
        Box::pin(async { Err(unavailable()) })
    }
}

#[derive(Clone)]
pub struct MarketingConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl MarketingConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn marketing_actor(current: &AuthorizedWorkContext) -> Result<MarketingLeadActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(MarketingLeadActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: workspace.company_id.as_str().to_owned(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        assignment_id: assignment.id.as_str().to_owned(),
        role: Some(assignment.role.clone()),
        authority_revision: Some(selected.revision()),
        assignment_version: Some(assignment.version),
        permissions: current.command_iam_metadata().permissions.clone(),
        modules: current.modules().to_vec(),
        expires_at: current.expires_at(),
    })
}

fn source_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "MARKETING_SOURCE_CHANGED",
        "The original Marketing source changed before the command completed.",
    )
}

fn source_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "MARKETING_SOURCE_NOT_FOUND",
        "The original Marketing source is unavailable in this workspace.",
    )
}

/// Same-unit purchase admission for one Marketing command. `purpose` selects
/// new-work, settle-existing or replay history admission.
async fn marketing_purchase_witness(
    connection: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    catalogue: &CommercialCatalogue,
    purpose: CapabilityPurpose,
    now: TimestampMillis,
) -> Result<PurchaseWitness, Error> {
    let mut purchases =
        PgCompanyPurchaseReader::new(holding.clone(), company.clone(), connection, catalogue);
    let inspection = purchases
        .inspect(
            &PurchaseRequest {
                capabilities: vec!["sales.marketing".into()],
                purpose,
                mode: PackageEnforcementMode::Enforced,
            },
            now,
        )
        .await
        .map_err(business)?;
    let witness = inspection.witness.clone();
    inspection.require_allowed()?;
    Ok(witness)
}

/// Revalidate the same-unit purchase witness after the owner command so the
/// command cannot commit on stale admission facts.
async fn revalidate_marketing_purchase(
    connection: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    catalogue: &CommercialCatalogue,
    witness: &PurchaseWitness,
    now: TimestampMillis,
) -> Result<(), Error> {
    PgCompanyPurchaseReader::new(holding.clone(), company.clone(), connection, catalogue)
        .revalidate(witness, now)
        .await
        .map_err(business)?
        .require_allowed()?;
    Ok(())
}

async fn candidate_source_witness(
    connection: &mut PgConnection,
    actor: &MarketingLeadActor,
    candidate_id: &str,
) -> Result<String, Error> {
    let branch = actor.branch_id.as_ref().map(|value| value.as_str());
    let department = actor.department_id.as_ref().map(|value| value.as_str());
    sqlx::query_scalar(
        "SELECT (to_jsonb(candidate) - 'status' - 'crm_lead_id' - 'reviewed_by' - 'version' - 'updated_at')::text \
         FROM marketing_lead_candidates candidate \
         WHERE candidate.id=$1 AND candidate.company_id=$2 \
         AND ($3::text IS NULL OR candidate.branch_id IS NULL OR candidate.branch_id=$3) \
         AND ($4::text IS NULL OR candidate.department_id IS NULL OR candidate.department_id=$4)",
    )
    .bind(candidate_id)
    .bind(&actor.company_id)
    .bind(branch)
    .bind(department)
    .fetch_optional(&mut *connection)
    .await?
    .ok_or_else(|| source_not_found().into())
}

async fn command_replayed(
    connection: &mut PgConnection,
    key: &CommandKey,
    actor: &MarketingLeadActor,
    operation: &str,
    record_id: &str,
) -> Result<bool, Error> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM command_receipts receipt \
         WHERE receipt.command_id=$1 AND receipt.operation=$2 \
         AND receipt.record_id=$3 AND receipt.actor_id=$4 \
         AND receipt.authority_json::jsonb->>'companyId'=$5)",
    )
    .bind(key.as_str())
    .bind(operation)
    .bind(record_id)
    .bind(&actor.user_id)
    .bind(&actor.company_id)
    .fetch_one(&mut *connection)
    .await?)
}

/// Witness of the Company's other Marketing campaigns. Campaign creation is
/// append-only (`marketing_campaigns` has no update path), so an unchanged set
/// outside the new deterministic id proves no concurrent campaign command
/// interleaved with this one.
async fn campaign_set_witness(
    connection: &mut PgConnection,
    actor: &MarketingLeadActor,
    created_id: &str,
) -> Result<String, Error> {
    Ok(sqlx::query_scalar(
        "SELECT COALESCE(jsonb_agg(jsonb_build_array(campaign.id, campaign.code, campaign.name) ORDER BY campaign.id), '[]'::jsonb)::text \
         FROM marketing_campaigns campaign \
         WHERE campaign.company_id=$1 AND campaign.id<>$2",
    )
    .bind(&actor.company_id)
    .bind(created_id)
    .fetch_one(&mut *connection)
    .await?)
}

/// Witness of the Company's other Marketing assets. Asset creation only
/// appends, and the retention guard rejects edits to the source columns, so an
/// unchanged identity/version/status set outside the new deterministic id
/// proves no concurrent asset command interleaved with this one.
async fn asset_set_witness(
    connection: &mut PgConnection,
    actor: &MarketingLeadActor,
    created_id: &str,
) -> Result<String, Error> {
    Ok(sqlx::query_scalar(
        "SELECT COALESCE(jsonb_agg(jsonb_build_array(asset.id, asset.version, asset.status) ORDER BY asset.id), '[]'::jsonb)::text \
         FROM marketing_assets asset \
         WHERE asset.company_id=$1 AND asset.id<>$2",
    )
    .bind(&actor.company_id)
    .bind(created_id)
    .fetch_one(&mut *connection)
    .await?)
}

/// Exact approved form source consumed by candidate capture, mirroring the
/// owner read (`kind='form'`, `status='active'`, exact version). A missing form
/// stays `None` so the owner use case keeps its own `MARKETING_FORM_NOT_READY`
/// denial instead of this witness inventing a different error.
async fn candidate_form_witness(
    connection: &mut PgConnection,
    actor: &MarketingLeadActor,
    form_id: &str,
    form_version: i64,
) -> Result<Option<String>, Error> {
    Ok(sqlx::query_scalar(
        "SELECT configuration_json FROM marketing_assets \
         WHERE id=$1 AND company_id=$2 AND kind='form' AND status='active' AND version=$3",
    )
    .bind(form_id)
    .bind(&actor.company_id)
    .bind(form_version)
    .fetch_optional(&mut *connection)
    .await?)
}

/// Witness of the immutable Marketing asset source reviewed by activate,
/// reject or retire. Status, reviewer, version and update time are the only
/// mutable columns and are intentionally excluded.
async fn asset_source_witness(
    connection: &mut PgConnection,
    actor: &MarketingLeadActor,
    asset_id: &str,
) -> Result<Option<String>, Error> {
    Ok(sqlx::query_scalar(
        "SELECT (to_jsonb(asset) - 'status' - 'reviewed_by' - 'version' - 'updated_at')::text \
         FROM marketing_assets asset \
         WHERE asset.id=$1 AND asset.company_id=$2",
    )
    .bind(asset_id)
    .bind(&actor.company_id)
    .fetch_optional(&mut *connection)
    .await?)
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

pub struct MarketingService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: MarketingConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> MarketingService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: MarketingConfiguration,
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
            permission: "records.read".into(),
            module: Some("marketing".into()),
            destination: "marketing".into(),
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

    fn write_access() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: "records.write".into(),
            module: Some("marketing".into()),
            destination: "marketing".into(),
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
        let company = self.configuration.company.clone();
        move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = true;
            Ok(request)
        }
    }

    /// Record one native Marketing engagement event. The current Account
    /// session, selected Company/branch/department, current IAM, purchase
    /// admission, source witness, owner version guard, receipt and history stay
    /// in one serializable Holding transaction.
    pub async fn record_engagement<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        candidate_id: String,
        key: CommandKey,
        action: MarketingLeadActionInput,
    ) -> Result<MarketingCommandReceipt, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.command_input(input),
                Self::write_access(),
                MarketingSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let candidate_id = candidate_id.clone();
                    let key = key.clone();
                    let action = action.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.write")?;
                        let actor = marketing_actor(current)?;
                        let now = clock.now();
                        let catalogue = current_package_catalogue()?;
                        let replayed =
                            command_replayed(connection, &key, &actor, "marketing.candidate.engagement", &candidate_id).await?;
                        let purpose = if replayed {
                            CapabilityPurpose::History
                        } else {
                            CapabilityPurpose::SettleExisting
                        };
                        let witness = {
                            let mut purchases = PgCompanyPurchaseReader::new(
                                holding.clone(),
                                company.clone(),
                                connection,
                                &catalogue,
                            );
                            let inspection = purchases
                                .inspect(
                                    &PurchaseRequest {
                                        capabilities: vec!["sales.marketing".into()],
                                        purpose,
                                        mode: PackageEnforcementMode::Enforced,
                                    },
                                    now,
                                )
                                .await
                                .map_err(business)?;
                            let witness = inspection.witness.clone();
                            inspection.require_allowed()?;
                            witness
                        };
                        let before =
                            candidate_source_witness(connection, &actor, &candidate_id).await?;
                        let receipt = record_marketing_engagement(
                            &actor,
                            &candidate_id,
                            &action,
                            key.as_str(),
                            &mut PgMarketingLead,
                            connection,
                            now,
                        )
                        .await
                        .map_err(business)?;
                        let after =
                            candidate_source_witness(connection, &actor, &candidate_id).await?;
                        if before != after {
                            return Err(source_changed().into());
                        }
                        PgCompanyPurchaseReader::new(
                            holding.clone(),
                            company.clone(),
                            connection,
                            &catalogue,
                        )
                        .revalidate(&witness, now)
                        .await
                        .map_err(business)?
                        .require_allowed()?;
                        Ok(receipt)
                    })
                },
            )
            .await
    }

    /// Create one native Marketing campaign. The current Account session,
    /// selected Company workspace, current IAM, same-unit new-work purchase
    /// admission, Company campaign witness, owner command and receipt stay in
    /// one serializable Holding transaction.
    pub async fn create_campaign<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        key: CommandKey,
        campaign: MarketingCampaignCreateInput,
    ) -> Result<MarketingCommandReceipt, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.command_input(input),
                Self::write_access(),
                MarketingSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let key = key.clone();
                    let campaign = campaign.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.write")?;
                        let actor = marketing_actor(current)?;
                        let now = clock.now();
                        let catalogue = current_package_catalogue()?;
                        let record_id = format!("campaign-{}", key.as_str());
                        let replayed = command_replayed(
                            connection,
                            &key,
                            &actor,
                            "marketing.campaign.create",
                            &record_id,
                        )
                        .await?;
                        let purpose = if replayed {
                            CapabilityPurpose::History
                        } else {
                            CapabilityPurpose::NewWork
                        };
                        let witness = marketing_purchase_witness(
                            connection,
                            &holding,
                            &company,
                            &catalogue,
                            purpose,
                            now,
                        )
                        .await?;
                        let before = campaign_set_witness(connection, &actor, &record_id).await?;
                        let receipt = create_marketing_campaign(
                            &actor,
                            &campaign,
                            key.as_str(),
                            &mut PgMarketingLead,
                            connection,
                            now,
                        )
                        .await
                        .map_err(business)?;
                        let after = campaign_set_witness(connection, &actor, &record_id).await?;
                        if before != after {
                            return Err(source_changed().into());
                        }
                        revalidate_marketing_purchase(
                            connection,
                            &holding,
                            &company,
                            &catalogue,
                            &witness,
                            now,
                        )
                        .await?;
                        Ok(receipt)
                    })
                },
            )
            .await
    }

    /// Create one native Marketing lead candidate from an approved form. The
    /// form configuration witness, same-unit new-work purchase admission,
    /// owner command, engagement and receipt stay in one serializable Holding
    /// transaction.
    pub async fn create_candidate<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        key: CommandKey,
        candidate: MarketingLeadCandidateInput,
    ) -> Result<MarketingCommandReceipt, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.command_input(input),
                Self::write_access(),
                MarketingSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let key = key.clone();
                    let candidate = candidate.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.write")?;
                        let actor = marketing_actor(current)?;
                        let now = clock.now();
                        let catalogue = current_package_catalogue()?;
                        let record_id = format!("marketing-candidate-{}", key.as_str());
                        let replayed = command_replayed(
                            connection,
                            &key,
                            &actor,
                            "marketing.candidate.create",
                            &record_id,
                        )
                        .await?;
                        let purpose = if replayed {
                            CapabilityPurpose::History
                        } else {
                            CapabilityPurpose::NewWork
                        };
                        let witness = marketing_purchase_witness(
                            connection,
                            &holding,
                            &company,
                            &catalogue,
                            purpose,
                            now,
                        )
                        .await?;
                        let before = candidate_form_witness(
                            connection,
                            &actor,
                            &candidate.form_id,
                            candidate.form_version,
                        )
                        .await?;
                        let receipt = create_marketing_candidate(
                            &actor,
                            &candidate,
                            key.as_str(),
                            &mut PgMarketingLead,
                            connection,
                            now,
                        )
                        .await
                        .map_err(business)?;
                        let after = candidate_form_witness(
                            connection,
                            &actor,
                            &candidate.form_id,
                            candidate.form_version,
                        )
                        .await?;
                        if before != after {
                            return Err(source_changed().into());
                        }
                        revalidate_marketing_purchase(
                            connection,
                            &holding,
                            &company,
                            &catalogue,
                            &witness,
                            now,
                        )
                        .await?;
                        Ok(receipt)
                    })
                },
            )
            .await
    }

    /// Draft one native Marketing asset. The Company asset witness, same-unit
    /// new-work purchase admission and owner command stay in one serializable
    /// Holding transaction.
    pub async fn create_asset<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        key: CommandKey,
        asset: MarketingAssetCreateInput,
    ) -> Result<MarketingCommandReceipt, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.command_input(input),
                Self::write_access(),
                MarketingSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let key = key.clone();
                    let asset = asset.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.write")?;
                        let actor = marketing_actor(current)?;
                        let now = clock.now();
                        let catalogue = current_package_catalogue()?;
                        let record_id = format!("marketing-{}-{}", asset.kind, key.as_str());
                        let replayed = command_replayed(
                            connection,
                            &key,
                            &actor,
                            "marketing.asset.create",
                            &record_id,
                        )
                        .await?;
                        let purpose = if replayed {
                            CapabilityPurpose::History
                        } else {
                            CapabilityPurpose::NewWork
                        };
                        let witness = marketing_purchase_witness(
                            connection,
                            &holding,
                            &company,
                            &catalogue,
                            purpose,
                            now,
                        )
                        .await?;
                        let before = asset_set_witness(connection, &actor, &record_id).await?;
                        let receipt = create_marketing_asset(
                            &actor,
                            &asset,
                            key.as_str(),
                            &mut PgMarketingLead,
                            connection,
                            now,
                        )
                        .await
                        .map_err(business)?;
                        let after = asset_set_witness(connection, &actor, &record_id).await?;
                        if before != after {
                            return Err(source_changed().into());
                        }
                        revalidate_marketing_purchase(
                            connection,
                            &holding,
                            &company,
                            &catalogue,
                            &witness,
                            now,
                        )
                        .await?;
                        Ok(receipt)
                    })
                },
            )
            .await
    }

    /// Review one native Marketing asset. Activation is new work; reject and
    /// retire settle the existing reviewed asset. Replays use history
    /// admission. The immutable asset source witness, purchase admission and
    /// owner transition stay in one serializable Holding transaction.
    pub async fn review_asset<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        asset_id: String,
        key: CommandKey,
        review: MarketingAssetReviewInput,
    ) -> Result<MarketingCommandReceipt, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.command_input(input),
                Self::write_access(),
                MarketingSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let asset_id = asset_id.clone();
                    let key = key.clone();
                    let review = review.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.write")?;
                        let actor = marketing_actor(current)?;
                        let now = clock.now();
                        let catalogue = current_package_catalogue()?;
                        let operation = match review.action.as_str() {
                            "activate" => "marketing.asset.activate",
                            "reject" => "marketing.asset.reject",
                            "retire" => "marketing.asset.retire",
                            _ => return Err(command_body_invalid().into()),
                        };
                        let replayed =
                            command_replayed(connection, &key, &actor, operation, &asset_id)
                                .await?;
                        let purpose = if replayed {
                            CapabilityPurpose::History
                        } else if review.action == "activate" {
                            CapabilityPurpose::NewWork
                        } else {
                            CapabilityPurpose::SettleExisting
                        };
                        let witness = marketing_purchase_witness(
                            connection,
                            &holding,
                            &company,
                            &catalogue,
                            purpose,
                            now,
                        )
                        .await?;
                        let before =
                            asset_source_witness(connection, &actor, &asset_id).await?;
                        let receipt = review_marketing_asset(
                            &actor,
                            &asset_id,
                            &review,
                            key.as_str(),
                            &mut PgMarketingLead,
                            connection,
                            now,
                        )
                        .await
                        .map_err(business)?;
                        let after = asset_source_witness(connection, &actor, &asset_id).await?;
                        if before != after {
                            return Err(source_changed().into());
                        }
                        revalidate_marketing_purchase(
                            connection,
                            &holding,
                            &company,
                            &catalogue,
                            &witness,
                            now,
                        )
                        .await?;
                        Ok(receipt)
                    })
                },
            )
            .await
    }

    pub async fn read<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
    ) -> Result<MarketingCampaignHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                MarketingSources,
                move |_connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        // No trusted Commerce mode or same-unit purchase
                        // witness has been bound to this composition yet.
                        Err(unavailable().into())
                    })
                },
            )
            .await
    }

    pub async fn lead_read<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        candidate: Option<String>,
    ) -> Result<MarketingLeadHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                MarketingSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let candidate = candidate.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        let now = clock.now();
                        let (witness, revision) = {
                            let catalogue = current_package_catalogue()?;
                            let mut purchase = PgCompanyPurchaseReader::new(
                                holding.clone(),
                                company.clone(),
                                connection,
                                &catalogue,
                            );
                            let inspection = purchase
                                .inspect(
                                    &PurchaseRequest {
                                        capabilities: vec!["sales.marketing".into()],
                                        purpose: CapabilityPurpose::History,
                                        mode: PackageEnforcementMode::Enforced,
                                    },
                                    now,
                                )
                                .await
                                .map_err(business)?;
                            inspection.require_allowed()?;
                            (inspection.witness.clone(), inspection.revision.clone())
                        };
                        let selected = current.selection();
                        let workspace = selected.workspace();
                        let assignment = selected.assignment();
                        let actor = MarketingLeadActor {
                            user_id: current.user_id().as_str().to_owned(),
                            company_id: workspace.company_id.as_str().to_owned(),
                            branch_id: workspace.branch_id.clone(),
                            department_id: workspace.department_id.clone(),
                            assignment_id: assignment.id.as_str().to_owned(),
                            role: Some(assignment.role.clone()),
                            authority_revision: Some(selected.revision()),
                            assignment_version: Some(assignment.version),
                            permissions: current.command_iam_metadata().permissions.clone(),
                            modules: current.modules().to_vec(),
                            expires_at: current.expires_at(),
                        };
                        let mut repository = PgMarketingLead;
                        let data = match candidate.as_deref() {
                            Some(id) => {
                                let detail =
                                    lead_detail(&actor, id, &mut repository, connection, now)
                                        .await
                                        .map_err(business)?;
                                serde_json::json!({
                                    "candidate": detail.candidate,
                                    "history": detail.history,
                                    "purchase": {
                                        "newWork": false,
                                        "settleExisting": false,
                                        "history": true,
                                        "revision": revision,
                                    }
                                })
                            }
                            None => {
                                let assets =
                                    asset_profiles(&actor, &mut repository, connection, now)
                                        .await
                                        .map_err(business)?;
                                serde_json::to_value(assets).map_err(|_| unavailable())?
                            }
                        };
                        let catalogue = current_package_catalogue()?;
                        PgCompanyPurchaseReader::new(
                            holding.clone(),
                            company.clone(),
                            connection,
                            &catalogue,
                        )
                        .revalidate(&witness, now)
                        .await
                        .map_err(business)?
                        .require_allowed()?;
                        Ok(MarketingLeadHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct MarketingBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<MarketingService<C>>,
}

impl<C: Clock + Send + Sync + 'static> MarketingBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<MarketingService<C>>,
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
                company_id: self.service.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: false,
            },
        ))
    }

    fn command_context(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found());
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        let field = |name: &'static str| -> Result<&str, BusinessError> {
            let mut values = headers.get_all(name).iter();
            let first = values.next();
            if values.next().is_some() {
                return Err(BusinessError::invalid(
                    "MARKETING_CONTEXT_REQUIRED",
                    "Use one current work context.",
                ));
            }
            first
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    BusinessError::invalid(
                        "MARKETING_CONTEXT_REQUIRED",
                        "Use the current work revision and assignment.",
                    )
                })
        };
        let revision = field("x-erpx-context-revision")?
            .parse::<i64>()
            .map_err(|_| {
                BusinessError::invalid(
                    "MARKETING_CONTEXT_REQUIRED",
                    "Use the current work revision.",
                )
            })?;
        if revision < 0 {
            return Err(BusinessError::invalid(
                "MARKETING_CONTEXT_REQUIRED",
                "Use the current work revision.",
            ));
        }
        let assignment_id = AssignmentId::new(field("x-erpx-context-assignment")?)?;
        let assignment_version = field("x-erpx-context-assignment-version")?
            .parse::<i64>()
            .map_err(|_| {
                BusinessError::invalid(
                    "MARKETING_CONTEXT_REQUIRED",
                    "Use the current assignment version.",
                )
            })?;
        if assignment_version < 1 {
            return Err(BusinessError::invalid(
                "MARKETING_CONTEXT_REQUIRED",
                "Use the current assignment version.",
            ));
        }
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.service.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: Some(ExpectedWorkSelection {
                    revision,
                    assignment_id,
                    assignment_version,
                }),
                mutation: true,
            },
        ))
    }
}

impl<C: Clock + Send + Sync + 'static> MarketingCampaignHttpBackend for MarketingBackend<C> {
    fn read<'a>(
        &'a self,
        request: MarketingCampaignHttpReadRequest<'a>,
    ) -> MarketingCampaignHttpFuture<'a, MarketingCampaignHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .read(request.host, proof, move || Ok(selection))
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> MarketingLeadHttpBackend for MarketingBackend<C> {
    fn read<'a>(
        &'a self,
        request: MarketingLeadHttpReadRequest<'a>,
    ) -> MarketingLeadHttpFuture<'a, MarketingLeadHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .lead_read(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.candidate_id.map(str::to_owned),
                )
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> MarketingLeadCommandHttpBackend for MarketingBackend<C> {
    fn record_engagement<'a>(
        &'a self,
        request: MarketingLeadCommandHttpRequest<'a>,
    ) -> MarketingLeadCommandHttpFuture<'a> {
        Box::pin(async move {
            let (proof, selection) = self.command_context(request.headers)?;
            let input: MarketingLeadActionInput = serde_json::from_value(request.body.clone())
                .map_err(|_| crate::marketing::command_request_invalid())?;
            if input.action != "record-engagement" {
                return Err(BusinessError::invalid(
                    "MARKETING_COMMAND_NOT_NATIVE",
                    "Only the native Marketing engagement command is available.",
                ));
            }
            let receipt = self
                .service
                .record_engagement(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.candidate_id.to_owned(),
                    request.key.clone(),
                    input,
                )
                .await
                .map_err(business)?;
            receipt_reply(receipt)
        })
    }

    fn create_campaign<'a>(
        &'a self,
        request: MarketingCommandHttpRequest<'a>,
    ) -> MarketingLeadCommandHttpFuture<'a> {
        Box::pin(async move {
            let (proof, selection) = self.command_context(request.headers)?;
            let campaign: MarketingCampaignCreateInput =
                serde_json::from_value(request.body.clone()).map_err(|_| command_body_invalid())?;
            let receipt = self
                .service
                .create_campaign(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.key.clone(),
                    campaign,
                )
                .await
                .map_err(business)?;
            receipt_reply(receipt)
        })
    }

    fn create_candidate<'a>(
        &'a self,
        request: MarketingCommandHttpRequest<'a>,
    ) -> MarketingLeadCommandHttpFuture<'a> {
        Box::pin(async move {
            let (proof, selection) = self.command_context(request.headers)?;
            let candidate: MarketingLeadCandidateInput =
                serde_json::from_value(request.body.clone()).map_err(|_| command_body_invalid())?;
            let receipt = self
                .service
                .create_candidate(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.key.clone(),
                    candidate,
                )
                .await
                .map_err(business)?;
            receipt_reply(receipt)
        })
    }

    fn create_asset<'a>(
        &'a self,
        request: MarketingCommandHttpRequest<'a>,
    ) -> MarketingLeadCommandHttpFuture<'a> {
        Box::pin(async move {
            let (proof, selection) = self.command_context(request.headers)?;
            let asset: MarketingAssetCreateInput =
                serde_json::from_value(request.body.clone()).map_err(|_| command_body_invalid())?;
            let receipt = self
                .service
                .create_asset(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.key.clone(),
                    asset,
                )
                .await
                .map_err(business)?;
            receipt_reply(receipt)
        })
    }

    fn review_asset<'a>(
        &'a self,
        request: MarketingAssetReviewHttpRequest<'a>,
    ) -> MarketingLeadCommandHttpFuture<'a> {
        Box::pin(async move {
            let (proof, selection) = self.command_context(request.headers)?;
            let review: MarketingAssetReviewInput =
                serde_json::from_value(request.body.clone()).map_err(|_| command_body_invalid())?;
            let receipt = self
                .service
                .review_asset(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.asset_id.to_owned(),
                    request.key.clone(),
                    review,
                )
                .await
                .map_err(business)?;
            receipt_reply(receipt)
        })
    }
}

fn receipt_reply(receipt: MarketingCommandReceipt) -> Result<serde_json::Value, BusinessError> {
    let value = serde_json::to_value(receipt).map_err(|_| unavailable())?;
    Ok(serde_json::json!({ "data": value }))
}

fn command_body_invalid() -> BusinessError {
    BusinessError::invalid(
        "MARKETING_COMMAND_REQUEST_INVALID",
        "Use one bounded Marketing command.",
    )
}

fn command_request_invalid() -> BusinessError {
    BusinessError::invalid(
        "MARKETING_COMMAND_REQUEST_INVALID",
        "Use one bounded Marketing engagement command.",
    )
}

#[allow(dead_code)]
fn _router(
    targets: Vec<
        erpx_server_marketing::public::campaign_attribution_http::MarketingCampaignHttpTarget,
    >,
) -> Result<axum::Router, BusinessError> {
    erpx_server_marketing::public::campaign_attribution_http::marketing_campaign_http_router(
        targets,
    )
}
