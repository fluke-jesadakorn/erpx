//! Field's raw Service v1 observation, composed from owning source ports.
//! This is a technical source witness, not Commercial/IAM/review authority or
//! the complete five-owner settlement. The consuming grant owns those gates.
use erpx_server_field_service::public::package_settlement::*;
use erpx_server_finance::public::field_service_workspace::*;
use erpx_server_inventory::public::field_service_consumption_finance::*;
use erpx_server_organization::public::field_service_workspace::*;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use std::collections::{BTreeMap, BTreeSet};
type Error = PgTransactionError<BusinessError>;

#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct NativeServicePackagePreview {
    pub comparison: erpx_server_commercial::public::purchases::GrantBatchPreview,
    pub request: erpx_server_commercial::public::purchases::GrantBatch,
    pub review: erpx_server_commercial::public::package_review::PackageReview,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct NativeServicePackageOverview {
    pub company_id: String,
    #[serde(flatten)]
    pub facts: erpx_server_commercial::public::purchases::CompanyPurchaseFacts,
    pub revision: String,
    pub packages: Vec<erpx_server_commercial::public::purchases::PackageDefinition>,
    pub billing_mode: &'static str,
    pub observed_at: i64,
}
pub async fn overview_service_packages<C: erpx_server_shared::public::Clock + Sync>(
    holding_connection: &mut PgConnection,
    control_connection: &mut PgConnection,
    authority:&erpx_server_identity_access::public::platform_commercial_authority::CurrentPlatformCommercialAuthority,
    clock: &C,
    holding: erpx_server_shared::public::HoldingId,
    company: CompanyId,
) -> Result<NativeServicePackageOverview, Error> {
    use erpx_server_commercial::public::purchases::*;
    require_operation(authority,erpx_server_identity_access::public::platform_commercial_authority::PlatformCommercialOperation::Read)?;
    lock_commercial_authority_sources(holding_connection).await?;
    erpx_server_organization::public::lock_hierarchy_sources(holding_connection).await?;
    let catalogue = current_package_catalogue()?;
    let mut policy = ServicePackageGrantPolicy::new(
        control_connection,
        authority,
        clock,
        holding.clone(),
        company.clone(),
    );
    policy
        .authorize(holding_connection, &holding, &company)
        .await?;
    let facts = PgCompanyPurchaseReader::new(
        holding.clone(),
        company.clone(),
        holding_connection,
        &catalogue,
    )
    .read(&company)
    .await?;
    policy
        .authorize(holding_connection, &holding, &company)
        .await?;
    Ok(NativeServicePackageOverview {
        company_id: company.as_str().into(),
        revision: package_grant_revision(company.as_str(), &facts.grants)?,
        facts,
        packages: catalogue.definitions().to_vec(),
        billing_mode: "sandbox",
        observed_at: clock.now().get(),
    })
}
/// Preview has no review-write callback. An accidental use for Apply fails
/// before a grant or review evidence can be staged.
struct PreviewOnlyReview;
impl erpx_server_commercial::public::purchases::PgPackageBatchReview for PreviewOnlyReview {
    async fn prepare_sources(
        &mut self,
        _: &mut PgConnection,
        _: &erpx_server_commercial::public::purchases::GrantChange,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn verify(
        &mut self,
        _: &mut PgConnection,
        _: &erpx_server_commercial::public::purchases::GrantBatchPreview,
    ) -> Result<(), Error> {
        Err(preview_only().into())
    }
    async fn retain(
        &mut self,
        _: &mut PgConnection,
        _: &erpx_server_commercial::public::purchases::GrantBatchPreview,
    ) -> Result<(), Error> {
        Err(preview_only().into())
    }
}
fn preview_only() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PLATFORM_COMMERCIAL_REQUIRED",
        "A current Commercial management and MFA lease is required to apply packages.",
    )
}
fn require_operation(
    authority:&erpx_server_identity_access::public::platform_commercial_authority::CurrentPlatformCommercialAuthority,
    operation:erpx_server_identity_access::public::platform_commercial_authority::PlatformCommercialOperation,
) -> Result<(), BusinessError> {
    if authority.operation() != operation {
        return Err(preview_only());
    }
    Ok(())
}
fn review_actor(
    authority:&erpx_server_identity_access::public::platform_commercial_authority::CurrentPlatformCommercialAuthority,
) -> erpx_server_commercial::public::package_review::PackageReviewActor {
    let a = authority.selected();
    erpx_server_commercial::public::package_review::PackageReviewActor {
        user_id: a.user_id.clone(),
        session_id: a.session_id.clone(),
        assignment_id: a.assignment_id.clone(),
        role: a.role.clone(),
        role_revision: a.role_revision,
    }
}
/// Both connections are already-bound native resources. Caller owns a fresh
/// SERIALIZABLE Holding unit and retains the current Control lease until it
/// commits/rolls back that unit; neither database is selected by an HTTP flag.
pub async fn preview_service_package_batch<C: erpx_server_shared::public::Clock + Sync>(
    holding_connection: &mut PgConnection,
    control_connection: &mut PgConnection,
    authority:&erpx_server_identity_access::public::platform_commercial_authority::CurrentPlatformCommercialAuthority,
    clock: &C,
    holding: erpx_server_shared::public::HoldingId,
    input: &erpx_server_commercial::public::purchases::GrantBatch,
    signer: &erpx_server_commercial::public::package_review::PackageLifecycleReviewSigner,
) -> Result<NativeServicePackagePreview, Error> {
    use erpx_server_commercial::public::purchases::*;
    require_operation(authority,erpx_server_identity_access::public::platform_commercial_authority::PlatformCommercialOperation::Read)?;
    let company = CompanyId::new(&input.company_id)?;
    let catalogue = current_package_catalogue()?;
    let mut policy = ServicePackageGrantPolicy::new(
        control_connection,
        authority,
        clock,
        holding.clone(),
        company.clone(),
    );
    let comparison = PgPackageGrantCommands::new(
        holding.clone(),
        company,
        holding_connection,
        &catalogue,
        &mut policy,
        clock,
    )
    .preview_batch(input, &mut PreviewOnlyReview)
    .await?;
    policy.require_current(holding_connection).await?;
    let review = signer.issue(
        &catalogue,
        review_actor(authority),
        holding.as_str(),
        input,
        &input.expected_revision,
        clock.now().get(),
    )?;
    Ok(NativeServicePackagePreview {
        comparison,
        request: input.clone(),
        review,
    })
}
struct ServicePackageReview<'a, C> {
    inner: erpx_server_commercial::public::package_review::PgPackageLifecycleReview<'a, C>,
    wrote: bool,
}
impl<C: erpx_server_shared::public::Clock + Sync>
    erpx_server_commercial::public::purchases::PgPackageBatchReview
    for ServicePackageReview<'_, C>
{
    async fn prepare_sources(
        &mut self,
        c: &mut PgConnection,
        input: &erpx_server_commercial::public::purchases::GrantChange,
    ) -> Result<(), Error> {
        self.inner.prepare_sources(c, input).await
    }
    async fn verify(
        &mut self,
        c: &mut PgConnection,
        preview: &erpx_server_commercial::public::purchases::GrantBatchPreview,
    ) -> Result<(), Error> {
        self.inner.verify(c, preview).await?;
        self.wrote = true;
        Ok(())
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        preview: &erpx_server_commercial::public::purchases::GrantBatchPreview,
    ) -> Result<(), Error> {
        // Includes the original Finance/Commercial deferred owner effects. Any
        // late source mutation remains rollbackable before the final guards.
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *c)
            .await?;
        self.inner.retain(c, preview).await
    }
}
#[allow(clippy::too_many_arguments)]
pub async fn apply_service_package_batch<C: erpx_server_shared::public::Clock + Sync>(
    holding_connection: &mut PgConnection,
    control_connection: &mut PgConnection,
    authority:&erpx_server_identity_access::public::platform_commercial_authority::CurrentPlatformCommercialAuthority,
    clock: &C,
    holding: erpx_server_shared::public::HoldingId,
    input: &erpx_server_commercial::public::purchases::GrantBatch,
    signer: &erpx_server_commercial::public::package_review::PackageLifecycleReviewSigner,
    token: &str,
) -> Result<erpx_server_commercial::public::purchases::GrantBatchPreview, Error> {
    use erpx_server_commercial::public::package_review::*;
    use erpx_server_commercial::public::purchases::*;
    require_operation(authority,erpx_server_identity_access::public::platform_commercial_authority::PlatformCommercialOperation::Apply)?;
    let company = CompanyId::new(&input.company_id)?;
    let catalogue = current_package_catalogue()?;
    let inner = PgPackageLifecycleReview::new(
        signer,
        clock,
        &catalogue,
        &review_actor(authority),
        holding.as_str(),
        input,
        token,
    )?;
    let mut review = ServicePackageReview {
        inner,
        wrote: false,
    };
    let mut policy = ServicePackageGrantPolicy::new(
        control_connection,
        authority,
        clock,
        holding.clone(),
        company.clone(),
    );
    let result = PgPackageGrantCommands::new(
        holding,
        company,
        holding_connection,
        &catalogue,
        &mut policy,
        clock,
    )
    .apply_batch(input, &mut review)
    .await?;
    policy.require_current(holding_connection).await?;
    if review.wrote {
        review.inner.retain(holding_connection, &result).await?;
    }
    authority
        .require_current(policy.control, clock.now().get())
        .await?;
    Ok(result)
}

/// The concrete Service-only Commercial policy. Control authority is lent by
/// native Account/IAM; the caller keeps that Control transaction open through
/// the Holding commit. Other package families must use their own native policy.
struct ServicePackageGrantPolicy<'a,C> {
    control:&'a mut PgConnection,
    authority:&'a erpx_server_identity_access::public::platform_commercial_authority::CurrentPlatformCommercialAuthority,
    clock:&'a C,
    holding:erpx_server_shared::public::HoldingId,
    company:CompanyId,
    company_image:Option<erpx_server_organization::public::OrganizationNode>,
    snapshots:BTreeMap<String,Box<ServiceSettlementSnapshot>>,
}
struct ServiceSettlementSnapshot {
    field: CurrentFieldPackageObligations,
    support: CurrentServiceSupportPackageObligations,
    inventory: CurrentServiceInventoryPackageObligations,
    assets: erpx_server_assets::public::field_service_workspace::CurrentServiceAssetSettlement,
    finance: CurrentServiceFinancePackageObligations,
    outstanding: i64,
}
impl ServiceSettlementSnapshot {
    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.finance.require_current(c, &self.field).await?;
        self.assets.require_current(c).await?;
        self.inventory.require_current(c, &self.field).await?;
        self.support.require_current(c, &self.field).await?;
        self.field.require_current(c).await
    }
}
impl<'a, C: erpx_server_shared::public::Clock + Sync> ServicePackageGrantPolicy<'a, C> {
    /// A Read lease cannot be promoted to an Apply lease by can_manage alone.
    fn new(
        control: &'a mut PgConnection,
        authority:&'a erpx_server_identity_access::public::platform_commercial_authority::CurrentPlatformCommercialAuthority,
        clock: &'a C,
        holding: erpx_server_shared::public::HoldingId,
        company: CompanyId,
    ) -> Self {
        Self {
            control,
            authority,
            clock,
            holding,
            company,
            company_image: None,
            snapshots: BTreeMap::new(),
        }
    }
    async fn require_current(&mut self, c: &mut PgConnection) -> Result<(), Error> {
        for snapshot in self.snapshots.values() {
            snapshot.require_current(c).await?;
        }
        self.authority
            .require_current(self.control, self.clock.now().get())
            .await
    }
}
impl<C: erpx_server_shared::public::Clock + Sync>
    erpx_server_commercial::public::purchases::PgPackageGrantPolicy
    for ServicePackageGrantPolicy<'_, C>
{
    async fn prepare_sources(
        &mut self,
        c: &mut PgConnection,
        input: &erpx_server_commercial::public::purchases::GrantChange,
    ) -> Result<(), Error> {
        if input.grant.company_id != self.company.as_str()
            || !matches!(
                (input.grant.package_id.as_str(), input.grant.package_version),
                ("service" | "service-parts-assets", 1)
            )
        {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "SERVICE_PACKAGE_NATIVE_SCOPE_REQUIRED",
                "This native command supports only the original Service package scope.",
            )
            .into());
        }
        lock_field_package_obligation_sources(c).await
    }
    async fn authorize(
        &mut self,
        c: &mut PgConnection,
        holding: &erpx_server_shared::public::HoldingId,
        company: &CompanyId,
    ) -> Result<erpx_server_shared::public::UserId, Error> {
        if holding != &self.holding || company != &self.company {
            return Err(BusinessError::invalid(
                "COMMERCIAL_COMPANY_MISMATCH",
                "The command belongs to another Company.",
            )
            .into());
        }
        // Authorization rechecks current Control/Company authority. The raw
        // owner witnesses are retained once and checked after all deferred
        // effects, rather than rescanning five owners on every batch callback.
        self.authority
            .require_current(self.control, self.clock.now().get())
            .await?;
        let context = erpx_server_organization::public::read_company_context(
            &mut erpx_server_organization::public::PgOrganizationReader::new(
                self.holding.clone(),
                c,
            ),
            company,
        )
        .await?;
        if self
            .company_image
            .as_ref()
            .is_some_and(|before| before != context.company())
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "COMMERCIAL_COMPANY_CHANGED",
                "The selected Company changed.",
            )
            .into());
        }
        self.company_image = Some(context.company().clone());
        Ok(erpx_server_shared::public::UserId::new(
            &self.authority.selected().user_id,
        )?)
    }
    async fn obligations(
        &mut self,
        c: &mut PgConnection,
        company: &CompanyId,
        package: &str,
    ) -> Result<i64, Error> {
        if company != &self.company || !matches!(package, "service" | "service-parts-assets") {
            return Err(BusinessError::invalid(
                "COMMERCIAL_COMPANY_MISMATCH",
                "The settlement belongs to another scope.",
            )
            .into());
        }
        if let Some(snapshot) = self.snapshots.get(package) {
            return Ok(snapshot.outstanding);
        }
        let field = Box::pin(read_field_package_obligations(
            c,
            company,
            package,
            1,
            self.clock.now().get(),
        ))
        .await?;
        let support = read_service_support_package_obligations(c, company, &field).await?;
        let inventory = read_service_inventory_package_obligations(c, company, &field).await?;
        let assets =
            erpx_server_assets::public::field_service_workspace::read_service_asset_settlement(
                c,
                company,
                &field.asset_sources(),
            )
            .await?;
        // This is original-source metadata under Platform Commercial authority,
        // never a selected Finance Work permission or a new Finance writer.
        let selected = self.authority.selected();
        let actor = erpx_server_finance::public::FinanceActor {
            holding_id: self.holding.clone(),
            company_id: company.clone(),
            user_id: erpx_server_shared::public::UserId::new(&selected.user_id)?,
            assignment_id: erpx_server_shared::public::AssignmentId::new(&selected.assignment_id)?,
        };
        let finance = Box::pin(read_service_finance_package_obligations(c, &actor, &field)).await?;
        let counts = [
            field.observation().outstanding,
            support.observation().outstanding,
            inventory.observation().outstanding,
            assets.facts().len(),
            finance.observation().outstanding,
        ];
        let outstanding = counts
            .into_iter()
            .try_fold(0i64, |sum, n| {
                i64::try_from(n).ok().and_then(|n| sum.checked_add(n))
            })
            .ok_or_else(|| {
                BusinessError::new(
                    FailureKind::Unavailable,
                    "SERVICE_PACKAGE_OBLIGATION_LIMIT",
                    "The settlement exceeds this bounded count.",
                )
            })?;
        let snapshot = Box::new(ServiceSettlementSnapshot {
            field,
            support,
            inventory,
            assets,
            finance,
            outstanding,
        });
        snapshot.require_current(c).await?;
        self.snapshots.insert(package.into(), snapshot);
        Ok(outstanding)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldPackageObligationObservation {
    pub company_id: String,
    pub package_id: String,
    pub module_id: &'static str,
    pub observed_at: i64,
    pub revision: String,
    pub outstanding: usize,
}
pub struct CurrentFieldPackageObligations {
    field: CurrentServicePackageSettlement,
    customers: CurrentServiceSettlementCustomers,
    locations: CurrentServiceSettlementLocations,
    consumptions: CurrentServiceSettlementConsumptions,
    selected_orders: BTreeSet<String>,
    valid_agreements: BTreeSet<String>,
    observation: FieldPackageObligationObservation,
}
impl CurrentFieldPackageObligations {
    /// Publish no raw identities, amounts or customer data in the observation.
    pub fn observation(&self) -> &FieldPackageObligationObservation {
        &self.observation
    }
    /// Exact original agreement/visit references, including terminal records.
    /// This carries no right to mutate or dispose of the customer's equipment.
    pub fn asset_sources(
        &self,
    ) -> Vec<erpx_server_assets::public::field_service_workspace::ServiceAssetSettlementSource>
    {
        use erpx_server_assets::public::field_service_workspace::ServiceAssetSettlementSource;
        self.field
            .data()
            .agreements
            .iter()
            .filter_map(|g| {
                g.asset_id
                    .as_ref()
                    .map(|asset| ServiceAssetSettlementSource {
                        id: g.id.clone(),
                        company_id: self.observation.company_id.clone(),
                        branch_id: g.branch_id.clone(),
                        department_id: g.department_id.clone(),
                        asset_id: asset.clone(),
                    })
            })
            .chain(self.field.data().visits.iter().filter_map(|v| {
                v.asset_id
                    .as_ref()
                    .map(|asset| ServiceAssetSettlementSource {
                        id: v.id.clone(),
                        company_id: self.observation.company_id.clone(),
                        branch_id: v.branch_id.clone(),
                        department_id: v.department_id.clone(),
                        asset_id: asset.clone(),
                    })
            }))
            .collect()
    }

    /// Inventory receives the original full Field Company scope for both
    /// Service base and addon settlement, not just personally visible visits.
    pub fn inventory_references(&self)->erpx_server_inventory::public::service_package_settlement::ServiceInventorySettlementReferences{
        use erpx_server_inventory::public::service_package_settlement::*;
        ServiceInventorySettlementReferences {
            orders: self
                .field
                .data()
                .visits
                .iter()
                .map(|v| ServiceInventorySettlementOrder {
                    id: v.id.clone(),
                    company_id: self.observation.company_id.clone(),
                    branch_id: v.branch_id.clone(),
                    department_id: v.department_id.clone(),
                    version: v.version,
                })
                .collect(),
            reservations: self
                .field
                .data()
                .reservations
                .iter()
                .map(|r| ServiceInventorySettlementReservation {
                    id: r.id.clone(),
                    order_id: r.order_id.clone(),
                    warehouse_id: r.warehouse_id.clone(),
                    product_id: r.product_id.clone(),
                    quantity_milli: r.quantity_milli,
                    consumed_milli: r.consumed_milli,
                    released_milli: r.released_milli,
                    digest: r.digest.clone(),
                })
                .collect(),
            events: self
                .field
                .data()
                .issue_events
                .iter()
                .map(|e| ServiceInventorySettlementEvent {
                    order_id: e.order_id.clone(),
                    version: e.version,
                    action: e.action.clone(),
                    details_json: e.details_json.clone(),
                })
                .collect(),
        }
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.customers.require_current(c).await?;
        self.consumptions.require_current(c).await?;
        self.locations.require_current(c).await?;
        self.field.require_current(c).await
    }
}
/// Acquire these leases as part of the full consuming operation's source
/// preparation, before its first selected-authority or data snapshot.
pub async fn lock_field_package_obligation_sources(c: &mut PgConnection) -> Result<(), Error> {
    erpx_server_finance::public::service_package_settlement::lock_service_finance_settlement(c)
        .await?;
    lock_service_workspace_customers(c).await?;
    erpx_server_commercial::public::purchases::lock_commercial_authority_sources(c).await?;
    erpx_server_workflow::public::record_origin::lock_record_origin_sources(c).await?;
    erpx_server_inventory::public::service_package_settlement::lock_service_inventory_settlement(c)
        .await?;
    erpx_server_support::public::field_service_workspace::lock_service_support_settlement(c)
        .await?;
    erpx_server_assets::public::field_service_workspace::lock_service_asset_settlement(c).await?;
    lock_service_workspace_locations(c).await?;
    lock_service_package_settlement(c).await?;
    erpx_server_files::public::record_version_receipts::lock_record_version_sources(c).await
}

pub async fn read_field_package_obligations(
    c: &mut PgConnection,
    company: &CompanyId,
    package_id: &str,
    package_version: i64,
    now: i64,
) -> Result<CurrentFieldPackageObligations, Error> {
    let package = match (package_id, package_version) {
        ("service", 1) => ServiceSettlementPackage::Base,
        ("service-parts-assets", 1) => ServiceSettlementPackage::PartsAssets,
        _ => {
            return Err(BusinessError::invalid(
                "SERVICE_PACKAGE_OBLIGATION_SCOPE_REQUIRED",
                "Use the original Service v1 package scope.",
            )
            .into());
        }
    };
    if !(0..=9_007_199_254_740_991).contains(&now) {
        return Err(BusinessError::invalid(
            "SERVICE_PACKAGE_OBLIGATION_SCOPE_REQUIRED",
            "Use an exact observation time.",
        )
        .into());
    }
    let field = read_service_package_settlement(c, company).await?;
    let customer_ids: Vec<_> = field
        .data()
        .agreements
        .iter()
        .map(|g| g.customer_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let location_ids: Vec<_> = field
        .data()
        .agreements
        .iter()
        .flat_map(|g| std::iter::once(g.branch_id.clone()).chain(g.department_id.clone()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let order_ids: Vec<_> = field.data().visits.iter().map(|v| v.id.clone()).collect();
    let customers = read_service_settlement_customers(c, company, &customer_ids).await?;
    let locations = read_service_settlement_locations(c, company, &location_ids).await?;
    let consumptions = read_service_settlement_consumptions(c, company, &order_ids).await?;
    let locations_by_id: BTreeMap<_, _> = locations
        .data()
        .iter()
        .map(|n| (n.id.as_str(), n))
        .collect();
    let customers_by_id: BTreeMap<_, _> = customers
        .data()
        .iter()
        .map(|p| (p.id.as_str(), p))
        .collect();
    let consumed: BTreeSet<_> = consumptions
        .order_ids()
        .iter()
        .map(String::as_str)
        .collect();
    let agreements: BTreeMap<_, _> = field
        .data()
        .agreements
        .iter()
        .map(|g| {
            // Original placement validity includes retained inactive nodes. Current
            // picker status and commercial access cannot make raw duties disappear.
            let valid = locations_by_id
                .get(g.branch_id.as_str())
                .is_some_and(|b| matches!(b.kind.as_str(), "branch" | "headquarters"))
                && g.department_id.as_deref().is_none_or(|id| {
                    locations_by_id.get(id).is_some_and(|d| {
                        d.kind == "department"
                            && d.branch_id.as_deref() == Some(g.branch_id.as_str())
                    })
                })
                && customers_by_id
                    .get(g.customer_id.as_str())
                    .is_some_and(|p| p.kind == "customer" && p.currency == g.currency);
            (g.id.as_str(), (g, valid))
        })
        .collect();
    let mut facts = Vec::new();
    for (g, valid) in agreements.values() {
        if let Some(fact) = agreement_settlement_fact(
            package,
            AgreementSettlementFacts {
                id: &g.id,
                version: g.version,
                state: &g.status,
                asset: g.asset_id.is_some(),
                source_valid: *valid,
                terminal_event: g.terminal_event,
            },
        ) {
            facts.push(fact);
        }
    }
    for v in &field.data().visits {
        let agreement = agreements.get(v.agreement_id.as_str());
        if let Some(fact) = visit_settlement_fact(
            package,
            VisitSettlementFacts {
                id: &v.id,
                version: v.version,
                state: &v.status,
                source_valid: v.agreement_relation_valid
                    && agreement.is_some_and(|(_, valid)| *valid),
                asset: v.asset_id.is_some(),
                agreement_asset: agreement.is_some_and(|(g, _)| g.asset_id.is_some()),
                reservation: v.reservation,
                consumption: consumed.contains(v.id.as_str()),
                outstanding_reservation: v.outstanding_reservation,
                terminal_event: v.terminal_event,
                billing_lineage: v.billing_lineage,
            },
        ) {
            facts.push(fact);
        }
    }
    facts.sort_by(|a, b| (a.kind, &a.id).cmp(&(b.kind, &b.id)));
    // Preserve the original JSON digest contract's field order and exact raw
    // facts. serde_json::Value would reorder keys and invalidate reviewed data.
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Revision<'a> {
        company_id: &'a str,
        package_id: &'a str,
        package_version: i64,
        facts: &'a [ServiceSettlementFact],
    }
    let bytes = serde_json::to_vec(&Revision {
        company_id: company.as_str(),
        package_id,
        package_version,
        facts: &facts,
    })
    .map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "SERVICE_PACKAGE_OBLIGATION_UNAVAILABLE",
            "The original Service settlement could not be read.",
        )
    })?;
    let observation = FieldPackageObligationObservation {
        company_id: company.as_str().into(),
        package_id: package_id.into(),
        module_id: "field_service",
        observed_at: now,
        revision: hex::encode(Sha256::digest(bytes)),
        outstanding: facts.iter().filter(|f| f.pending).count(),
    };
    let selected_orders = facts
        .iter()
        .filter(|f| f.kind == "visit")
        .map(|f| f.id.clone())
        .collect();
    let valid_agreements = agreements
        .iter()
        .filter_map(|(id, (_, valid))| valid.then_some((*id).to_owned()))
        .collect();
    let current = CurrentFieldPackageObligations {
        field,
        customers,
        locations,
        consumptions,
        selected_orders,
        valid_agreements,
        observation,
    };
    current.require_current(c).await?;
    Ok(current)
}

/// Native Support observations bind canonical owning-row digests rather than
/// returning private issue/ticket bodies. Revision remains an opaque SHA-256;
/// it is not a review token or a substitute for the grant's current source guard.
pub struct CurrentServiceSupportPackageObligations {
    support: erpx_server_support::public::field_service_workspace::CurrentServiceSupportSettlement,
    field_image: ServiceSettlementRows,
    valid_agreements: BTreeSet<String>,
    observation: FieldPackageObligationObservation,
}
impl CurrentServiceSupportPackageObligations {
    pub fn observation(&self) -> &FieldPackageObligationObservation {
        &self.observation
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        field: &CurrentFieldPackageObligations,
    ) -> Result<(), Error> {
        if field.observation.company_id != self.observation.company_id
            || field.observation.package_id != self.observation.package_id
            || field.field.data() != &self.field_image
            || field.valid_agreements != self.valid_agreements
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "SERVICE_SUPPORT_PACKAGE_SOURCE_CHANGED",
                "The original Field support source changed.",
            )
            .into());
        }
        field.require_current(c).await?;
        self.support.require_current(c).await
    }
}
pub async fn read_service_support_package_obligations(
    c: &mut PgConnection,
    company: &CompanyId,
    field: &CurrentFieldPackageObligations,
) -> Result<CurrentServiceSupportPackageObligations, Error> {
    use erpx_server_support::public::field_service_workspace::read_service_support_settlement;
    if field.observation.company_id != company.as_str() {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SERVICE_SUPPORT_PACKAGE_SOURCE_CHANGED",
            "The original Field Company changed.",
        )
        .into());
    }
    field.require_current(c).await?;
    let orders: Vec<_> = field
        .field
        .data()
        .visits
        .iter()
        .filter(|v| field.selected_orders.contains(&v.id))
        .collect();
    let known_tickets: Vec<_> = orders
        .iter()
        .map(|v| v.ticket_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let support = read_service_support_settlement(c, company, &known_tickets).await?;
    let tickets: BTreeMap<_, _> = support
        .data()
        .tickets
        .iter()
        .map(|t| (t.id.as_str(), t))
        .collect();
    let agreements: BTreeMap<_, _> = field
        .field
        .data()
        .agreements
        .iter()
        .map(|g| (g.id.as_str(), g))
        .collect();
    #[derive(Serialize)]
    struct Fact {
        kind: &'static str,
        id: String,
        fact: String,
    }
    let mut facts = Vec::new();
    for v in &orders {
        let agreement = agreements.get(v.agreement_id.as_str());
        let ticket = tickets.get(v.ticket_id.as_str()).filter(|t| {
            agreement.is_some()
                && t.company_id == company.as_str()
                && t.branch_id.as_deref() == Some(v.branch_id.as_str())
                && t.department_id == v.department_id
                && t.customer_id.as_deref() == agreement.map(|g| g.customer_id.as_str())
        });
        let expected_status = match v.status.as_str() {
            "billed" => "closed",
            "cancelled" => "cancelled",
            _ => "open",
        };
        if ticket.is_none_or(|t| {
            t.source_kind.as_deref() == Some("field_service")
                && (t.service_order_id.as_deref() != Some(v.id.as_str())
                    || t.service_agreement_id.as_deref() != Some(v.agreement_id.as_str())
                    || t.service_order_version != Some(v.version)
                    || t.status != expected_status)
        }) {
            #[derive(Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Source<'a> {
                contract: &'static str,
                row_digest: &'a str,
                source_valid: bool,
                customer_id: Option<&'a str>,
                currency: Option<&'a str>,
                agreement_asset_id: Option<&'a str>,
            }
            let image=Source{contract:erpx_server_support::public::field_service_workspace::SERVICE_PACKAGE_SETTLEMENT_CONTRACT,row_digest:&v.digest,source_valid:v.agreement_relation_valid&&field.valid_agreements.contains(&v.agreement_id),customer_id:agreement.map(|g|g.customer_id.as_str()),currency:agreement.map(|g|g.currency.as_str()),agreement_asset_id:agreement.and_then(|g|g.asset_id.as_deref())};
            facts.push(Fact {
                kind: "case-source",
                id: v.id.clone(),
                fact: serde_json::to_string(&image).map_err(|_| {
                    BusinessError::invalid(
                        "SERVICE_SUPPORT_PACKAGE_SOURCE_CHANGED",
                        "The Field source could not be read.",
                    )
                })?,
            });
        }
        if tickets
            .get(v.ticket_id.as_str())
            .is_some_and(|t| t.source_kind.as_deref() == Some("field_service"))
        {
            for alert in support
                .data()
                .alerts
                .iter()
                .filter(|a| a.ticket_id == v.ticket_id)
            {
                facts.push(Fact {
                    kind: "case-sla",
                    id: alert.id.clone(),
                    fact: alert.digest.clone(),
                });
            }
        }
    }
    if field.observation.package_id == "service" {
        for ticket in support.data().tickets.iter().filter(|t| {
            t.company_id == company.as_str() && t.source_kind.as_deref() == Some("field_service")
        }) {
            if !orders.iter().any(|v| {
                ticket.service_order_id.as_deref() == Some(v.id.as_str())
                    && v.ticket_id == ticket.id
            }) {
                facts.push(Fact {
                    kind: "unlinked-case",
                    id: ticket.id.clone(),
                    fact: ticket.digest.clone(),
                });
            }
        }
    }
    facts.sort_by(|a, b| (a.kind, &a.id).cmp(&(b.kind, &b.id)));
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Revision<'a> {
        company_id: &'a str,
        package_id: &'a str,
        package_version: i64,
        facts: &'a [Fact],
    }
    let bytes = serde_json::to_vec(&Revision {
        company_id: company.as_str(),
        package_id: &field.observation.package_id,
        package_version: 1,
        facts: &facts,
    })
    .map_err(|_| {
        BusinessError::invalid(
            "SERVICE_SUPPORT_PACKAGE_SOURCE_CHANGED",
            "The Support observation could not be read.",
        )
    })?;
    let observation = FieldPackageObligationObservation {
        company_id: company.as_str().into(),
        package_id: field.observation.package_id.clone(),
        module_id: "support",
        observed_at: field.observation.observed_at,
        revision: hex::encode(Sha256::digest(bytes)),
        outstanding: facts.len(),
    };
    let current = CurrentServiceSupportPackageObligations {
        support,
        field_image: field.field.data().clone(),
        valid_agreements: field.valid_agreements.clone(),
        observation,
    };
    current.require_current(c, field).await?;
    Ok(current)
}

pub struct CurrentServiceInventoryPackageObligations {
    inventory:erpx_server_inventory::public::service_package_settlement::CurrentServiceInventorySettlement,
    field_image:ServiceSettlementRows,
    observation:FieldPackageObligationObservation,
}
impl CurrentServiceInventoryPackageObligations {
    pub fn observation(&self) -> &FieldPackageObligationObservation {
        &self.observation
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        field: &CurrentFieldPackageObligations,
    ) -> Result<(), Error> {
        if field.observation.company_id != self.observation.company_id
            || field.observation.package_id != self.observation.package_id
            || field.field.data() != &self.field_image
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "SERVICE_INVENTORY_PACKAGE_SOURCE_CHANGED",
                "The original Field Inventory source changed.",
            )
            .into());
        }
        field.require_current(c).await?;
        self.inventory.require_current(c).await
    }
}
pub async fn read_service_inventory_package_obligations(
    c: &mut PgConnection,
    company: &CompanyId,
    field: &CurrentFieldPackageObligations,
) -> Result<CurrentServiceInventoryPackageObligations, Error> {
    use erpx_server_inventory::public::service_package_settlement::*;
    if field.observation.company_id != company.as_str() {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SERVICE_INVENTORY_PACKAGE_SOURCE_CHANGED",
            "The original Field Company changed.",
        )
        .into());
    }
    field.require_current(c).await?;
    let inventory = read_service_inventory_settlement(
        c,
        company,
        field.inventory_references(),
        erpx_server_files::public::record_version_receipts::RECORD_ATTACHMENT_VERSION_STEPS,
    )
    .await?;
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Revision<'a> {
        company_id: &'a str,
        package_id: &'a str,
        package_version: i64,
        contract: &'static str,
        source_revision: &'a str,
    }
    let bytes = serde_json::to_vec(&Revision {
        company_id: company.as_str(),
        package_id: &field.observation.package_id,
        package_version: 1,
        contract: CONTRACT,
        source_revision: &inventory.observation().revision,
    })
    .map_err(|_| {
        BusinessError::invalid(
            "SERVICE_INVENTORY_PACKAGE_SOURCE_CHANGED",
            "The Inventory observation could not be read.",
        )
    })?;
    let outstanding = usize::try_from(inventory.observation().outstanding).map_err(|_| {
        BusinessError::invalid(
            "SERVICE_INVENTORY_PACKAGE_SOURCE_CHANGED",
            "The Inventory observation could not be read.",
        )
    })?;
    let observation = FieldPackageObligationObservation {
        company_id: company.as_str().into(),
        package_id: field.observation.package_id.clone(),
        module_id: "inventory",
        observed_at: field.observation.observed_at,
        revision: hex::encode(Sha256::digest(bytes)),
        outstanding,
    };
    let current = CurrentServiceInventoryPackageObligations {
        inventory,
        field_image: field.field.data().clone(),
        observation,
    };
    current.require_current(c, field).await?;
    Ok(current)
}

/// The native Finance count retains the same concrete Field/Inventory/Workflow
/// provenance used by invoice and independent journal commands. Invalid sources
/// stay in the raw debt set; personal Finance page permissions are not borrowed.
struct ServicePartsSettlementOrigin {
    field:
        erpx_server_field_service::public::finance_lineage::CurrentFieldServicePartsFinanceLineage,
    origin: erpx_server_workflow::public::record_origin::PgRawRecordOrigin,
}
impl ServicePartsSettlementOrigin {
    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.field.require_current(c).await?;
        erpx_server_workflow::public::record_origin::verify_raw_record_origin(c, &self.origin).await
    }
}
pub struct CurrentServiceFinancePackageObligations {
    finance:erpx_server_finance::public::service_package_settlement::CurrentServiceFinanceSettlement,
    field_image:ServiceSettlementRows,
    invoices:Vec<erpx_server_field_service::public::finance_lineage::CurrentFieldServiceInvoiceFinanceLineage>,
    parts:Vec<ServicePartsSettlementOrigin>,
    observation:FieldPackageObligationObservation,
}
impl CurrentServiceFinancePackageObligations {
    pub fn observation(&self) -> &FieldPackageObligationObservation {
        &self.observation
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        field: &CurrentFieldPackageObligations,
    ) -> Result<(), Error> {
        if field.observation.company_id != self.observation.company_id
            || field.observation.package_id != self.observation.package_id
            || field.field.data() != &self.field_image
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "SERVICE_FINANCE_PACKAGE_SOURCE_CHANGED",
                "The original Field Finance source changed.",
            )
            .into());
        }
        field.require_current(c).await?;
        for source in &self.invoices {
            source.require_current(c).await?;
        }
        for source in &self.parts {
            source.require_current(c).await?;
        }
        self.finance.require_current(c).await
    }
}
pub async fn read_service_finance_package_obligations(
    c: &mut PgConnection,
    actor: &erpx_server_finance::public::FinanceActor,
    field: &CurrentFieldPackageObligations,
) -> Result<CurrentServiceFinancePackageObligations, Error> {
    use erpx_server_field_service::public::finance_lineage::*;
    use erpx_server_finance::public::service_package_settlement::*;
    use erpx_server_workflow::public::record_origin::{
        RawRecordOriginRequest, read_raw_record_origin,
    };
    if field.observation.company_id != actor.company_id.as_str() {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SERVICE_FINANCE_PACKAGE_SOURCE_CHANGED",
            "The original Field Company changed.",
        )
        .into());
    }
    field.require_current(c).await?;
    let mut invoices = Vec::new();
    let mut parts = Vec::new();
    let mut invoice_origins = Vec::new();
    let mut parts_origins = Vec::new();
    for b in &field.field.data().releases {
        let Some(key) = b.id.strip_prefix("field-billing-") else {
            continue;
        };
        let request = FieldServiceInvoiceFinanceRequest {
            company_id: actor.company_id.clone(),
            order_id: b.order_id.clone(),
            release_id: b.id.clone(),
            key: key.into(),
            actor_id: b.released_by.clone(),
            currency: b.currency.clone(),
            amount_units: b.amount_minor.to_string(),
            customer_id: b.customer_id.clone(),
            pricing_date: b.pricing_date.clone(),
        };
        let proof = match read_field_service_invoice_finance_lineage(c, request).await {
            Ok(value) => value,
            Err(PgTransactionError::Business(_)) => None,
            Err(error) => return Err(error),
        };
        if let Some(proof) = proof {
            invoice_origins.push(ServiceFinanceSettlementInvoiceOrigin {
                id: format!("field-invoice-{key}"),
                company_id: actor.company_id.as_str().into(),
                release_id: b.id.clone(),
                order_id: b.order_id.clone(),
                customer_id: b.customer_id.clone(),
                currency: b.currency.clone(),
                amount_minor: b.amount_minor,
                pricing_date: b.pricing_date.clone(),
            });
            invoices.push(proof);
        }
    }
    for p in field.consumptions.data() {
        if p.company_id != actor.company_id.as_str() {
            continue;
        }
        let Some(key) = p.id.strip_prefix("operational-part-") else {
            continue;
        };
        if [p.source_version, p.quantity_milli, p.value_minor]
            .iter()
            .any(|n| !(1..=9_007_199_254_740_991).contains(n))
        {
            continue;
        }
        let request = FieldServicePartsFinanceRequest {
            holding_id: actor.holding_id.clone(),
            company_id: actor.company_id.clone(),
            order_id: p.source_id.clone(),
            branch_id: p.branch_id.clone(),
            department_id: p.department_id.clone(),
            minimum_order_version: p.source_version,
            owner_key: key.into(),
            consumption_id: p.id.clone(),
            journal_id: p.journal_id.clone(),
            warehouse_id: p.warehouse_id.clone(),
            product_id: p.product_id.clone(),
            quantity_milli: p.quantity_milli,
            value_minor: p.value_minor,
            creator_user_id: p.created_by.clone(),
        };
        let proof = match read_field_service_parts_finance_lineage(c, request).await {
            Ok(value) => value,
            Err(PgTransactionError::Business(_)) => None,
            Err(error) => return Err(error),
        };
        let Some(proof) = proof else { continue };
        let origin = match read_raw_record_origin(
            c,
            RawRecordOriginRequest {
                command_key: key.into(),
                operation: "field.order.issue-part".into(),
                record_id: p.source_id.clone(),
                creator_user_id: p.created_by.clone(),
                company_id: actor.company_id.clone(),
            },
        )
        .await
        {
            Ok(value) => value,
            Err(PgTransactionError::Business(_)) => None,
            Err(error) => return Err(error),
        };
        if let Some(origin) = origin {
            parts_origins.push(ServiceFinanceSettlementPartsOrigin {
                company_id: p.company_id.clone(),
                journal_id: p.journal_id.clone(),
                consumption_id: p.id.clone(),
            });
            parts.push(ServicePartsSettlementOrigin {
                field: proof,
                origin,
            });
        }
    }

    let references = ServiceFinanceSettlementReferences {
        orders: field
            .field
            .data()
            .visits
            .iter()
            .map(|v| ServiceFinanceSettlementOrder {
                id: v.id.clone(),
                company_id: actor.company_id.as_str().into(),
            })
            .collect(),
        releases: field
            .field
            .data()
            .releases
            .iter()
            .map(|b| ServiceFinanceSettlementRelease {
                id: b.id.clone(),
                order_id: b.order_id.clone(),
                digest: b.digest.clone(),
            })
            .collect(),
        parts: field
            .consumptions
            .data()
            .iter()
            .map(|p| ServiceFinanceSettlementPart {
                id: p.id.clone(),
                company_id: p.company_id.clone(),
                source_id: p.source_id.clone(),
                journal_id: p.journal_id.clone(),
                currency: p.currency.clone(),
                value_minor: p.value_minor,
                digest: p.digest.clone(),
            })
            .collect(),
        invoice_origins,
        parts_origins,
    };
    let finance = read_service_finance_settlement(
        c,
        &actor.company_id,
        &field.observation.package_id,
        references,
    )
    .await?;
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Revision<'a> {
        company_id: &'a str,
        package_id: &'a str,
        package_version: i64,
        contract: &'static str,
        source_revision: &'a str,
    }
    let bytes = serde_json::to_vec(&Revision {
        company_id: actor.company_id.as_str(),
        package_id: &field.observation.package_id,
        package_version: 1,
        contract: CONTRACT,
        source_revision: &finance.observation().revision,
    })
    .map_err(|_| {
        BusinessError::invalid(
            "SERVICE_FINANCE_PACKAGE_SOURCE_CHANGED",
            "The Finance observation could not be read.",
        )
    })?;
    let outstanding = usize::try_from(finance.observation().outstanding).map_err(|_| {
        BusinessError::invalid(
            "SERVICE_FINANCE_PACKAGE_SOURCE_CHANGED",
            "The Finance observation could not be read.",
        )
    })?;
    let observation = FieldPackageObligationObservation {
        company_id: actor.company_id.as_str().into(),
        package_id: field.observation.package_id.clone(),
        module_id: "finance",
        observed_at: field.observation.observed_at,
        revision: hex::encode(Sha256::digest(bytes)),
        outstanding,
    };
    let current = CurrentServiceFinancePackageObligations {
        finance,
        field_image: field.field.data().clone(),
        invoices,
        parts,
        observation,
    };
    current.require_current(c, field).await?;
    Ok(current)
}
