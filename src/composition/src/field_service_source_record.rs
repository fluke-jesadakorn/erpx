//! Single Field source-record read with same-unit owner witnesses.
use crate::field_service_reservation_read::{FieldServiceRenderedContext, context};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseInspection, PurchaseRequest,
};
use erpx_server_identity_access::public::effective_authority::{
    AuthorizedWorkContext, HoldingWorkSourcePreparation,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use serde::Serialize;
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
#[derive(Clone, Copy, Debug)]
pub enum SourceRecordMode {
    Disabled,
    Shadow,
    Enforced,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcePurchaseObservation {
    pub event: &'static str,
    pub phase: &'static str,
    pub company_id: String,
    pub capability: String,
    pub purpose: &'static str,
    pub code: &'static str,
    pub purchase_revision: String,
}
/// The sink receives only scoped decision facts, never purchase or actor bodies.
pub fn observe_source_purchase(
    mode: SourceRecordMode,
    company: &CompanyId,
    inspection: &PurchaseInspection,
    sink: &mut impl FnMut(SourcePurchaseObservation),
) {
    if !matches!(mode, SourceRecordMode::Shadow) {
        return;
    }
    for access in &inspection.access {
        sink(SourcePurchaseObservation {
            event: "commercial.purchase.shadow",
            phase: "read",
            company_id: company.as_str().into(),
            capability: access.capability.clone(),
            purpose: match access.purpose {
                CapabilityPurpose::NewWork => "new-work",
                CapabilityPurpose::SettleExisting => "settle-existing",
                CapabilityPurpose::History => "history",
            },
            code: access.decision.code.as_code(),
            purchase_revision: inspection.revision.clone(),
        });
    }
}
fn write_observation(observation: SourcePurchaseObservation) {
    let mut output = std::io::stderr().lock();
    write_source_purchase_observation(&mut output, &observation);
}
/// Telemetry is best effort even if the diagnostic destination fails.
pub fn write_source_purchase_observation(
    output: &mut impl std::io::Write,
    observation: &SourcePurchaseObservation,
) {
    if serde_json::to_writer(&mut *output, observation).is_ok() {
        let _ = output.write_all(b"\n");
    }
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceSourceRecord {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    pub version: i64,
    pub title: String,
    pub status: String,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FieldServiceSourceRecordResponse {
    pub data: FieldServiceSourceRecord,
    pub context: FieldServiceRenderedContext,
}
#[derive(Clone)]
pub(crate) struct SourceRecordSources;
impl HoldingWorkSourcePreparation for SourceRecordSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            erpx_server_finance::public::field_service_workspace::lock_service_workspace_customers(
                c,
            )
            .await?;
            erpx_server_commercial::public::purchases::lock_commercial_authority_sources(c).await?;
            erpx_server_support::public::field_service_workspace::lock_service_workspace_tickets(c)
                .await?;
            erpx_server_assets::public::field_service_workspace::lock_service_workspace_assets(c)
                .await?;
            erpx_server_identity_access::public::field_service_workspace::lock_service_workspace_authority(c).await?;
            erpx_server_field_service::public::source_record::lock_source_record(c).await
        })
    }
}
fn missing() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "SOURCE_RECORD_NOT_FOUND",
        "Source record is unavailable in this work scope.",
    )
}
pub(crate) struct SourceRecordContext<'a, C> {
    pub actor: &'a AuthorizedWorkContext,
    pub holding: &'a HoldingId,
    pub company: &'a CompanyId,
    pub catalogue: &'a CommercialCatalogue,
    pub clock: &'a C,
}
pub(crate) async fn read<C: Clock>(
    c: &mut PgConnection,
    ctx: &SourceRecordContext<'_, C>,
    id: &str,
    expected: Option<f64>,
    mode: SourceRecordMode,
) -> Result<FieldServiceSourceRecordResponse, Error> {
    let SourceRecordContext {
        actor,
        holding,
        company,
        catalogue,
        clock,
    } = *ctx;
    use erpx_server_field_service::public::source_record::{
        FieldRecordLineage, check_version, read_source_record, require_source_lineage,
    };
    let mut shadow_purchases = Vec::new();
    if matches!(mode, SourceRecordMode::Shadow) {
        for purpose in [
            CapabilityPurpose::NewWork,
            CapabilityPurpose::SettleExisting,
            CapabilityPurpose::History,
        ] {
            let inspection =
                PgCompanyPurchaseReader::new(holding.clone(), company.clone(), c, catalogue)
                    .inspect(
                        &PurchaseRequest {
                            capabilities: vec!["service.orders".into()],
                            purpose,
                            mode: PackageEnforcementMode::Shadow,
                        },
                        clock.now(),
                    )
                    .await?;
            observe_source_purchase(mode, company, &inspection, &mut write_observation);
            shadow_purchases.push(inspection.witness);
        }
    }
    let s = actor.selection().workspace();
    let row = read_source_record(
        c,
        company,
        s.branch_id.as_ref().map(|x| x.as_str()),
        s.department_id.as_ref().map(|x| x.as_str()),
        id,
    )
    .await?;
    let f = row.data();
    let assignment = actor.selection().assignment();
    let node = assignment.scope_node_id.as_ref().map(|v| v.as_str());
    let scoped = match assignment.scope_kind {
        erpx_server_identity_access::public::work_context::WorkScopeKind::Holding => true,
        erpx_server_identity_access::public::work_context::WorkScopeKind::Branch => {
            node == Some(f.branch_id.as_str())
        }
        erpx_server_identity_access::public::work_context::WorkScopeKind::Department => {
            node.is_some() && node == f.department_id.as_deref()
        }
    };
    if !scoped {
        return Err(missing().into());
    }

    let mut purchase = None;
    if matches!(mode, SourceRecordMode::Enforced) {
        let p = PgCompanyPurchaseReader::new(holding.clone(), company.clone(), c, catalogue)
            .inspect(
                &PurchaseRequest {
                    capabilities: vec!["service.orders".into()],
                    purpose: CapabilityPurpose::History,
                    mode: PackageEnforcementMode::Enforced,
                },
                clock.now(),
            )
            .await?;
        let history = p.require_allowed().is_ok();
        purchase = Some(p.witness);
        if matches!(mode, SourceRecordMode::Enforced) {
            let g = f.agreement.as_ref().ok_or_else(missing)?;
            let customers=erpx_server_finance::public::field_service_workspace::read_service_workspace_customers(c,company,false,std::slice::from_ref(&g.customer_id)).await?;
            let tickets =
                erpx_server_support::public::field_service_source_record::read_field_source_ticket(
                    c,
                    company,
                    &f.ticket_id,
                )
                .await?;
            let assets =
                erpx_server_assets::public::field_service_workspace::read_service_workspace_assets(
                    c,
                    company,
                    None,
                    None,
                    false,
                    &f.asset_id.iter().cloned().collect::<Vec<_>>(),
                )
                .await?;
            let has_asset = f.asset_id.is_some() || g.asset_id.is_some();
            let mut part_purchase = None;
            let parts_history = if has_asset {
                let p =
                    PgCompanyPurchaseReader::new(holding.clone(), company.clone(), c, catalogue)
                        .inspect(
                            &PurchaseRequest {
                                capabilities: vec!["service.parts-assets".into()],
                                purpose: CapabilityPurpose::History,
                                mode: PackageEnforcementMode::Enforced,
                            },
                            clock.now(),
                        )
                        .await?;
                let yes = p.require_allowed().is_ok();
                part_purchase = Some(p.witness);
                yes
            } else {
                true
            };
            require_source_lineage(&FieldRecordLineage {
                agreement_scope: g.branch_id == f.branch_id && g.department_id == f.department_id,
                customer: customers
                    .data()
                    .display
                    .iter()
                    .any(|v| v.id == g.customer_id && v.kind == "customer"),
                ticket: tickets.data().is_some_and(|v| {
                    v.id == f.ticket_id
                        && v.customer_id.as_deref() == Some(g.customer_id.as_str())
                        && v.branch_id.as_deref() == Some(f.branch_id.as_str())
                        && v.department_id == f.department_id
                }),
                asset: f.asset_id.as_ref().is_none_or(|id| {
                    assets
                        .data()
                        .display
                        .iter()
                        .any(|v| &v.id == id && v.branch_id == f.branch_id)
                }),
                agreement_asset_matches: g
                    .asset_id
                    .as_ref()
                    .is_none_or(|id| f.asset_id.as_ref() == Some(id)),
                history,
                parts_history,
                has_asset,
            })?;
            customers.require_current(c).await?;
            tickets.require_current(c).await?;
            assets.require_current(c).await?;
            if let Some(p) = part_purchase {
                PgCompanyPurchaseReader::new(holding.clone(), company.clone(), c, catalogue)
                    .revalidate(&p, clock.now())
                    .await?;
            }
        }
    }
    check_version(f.version, expected)?;
    let data = FieldServiceSourceRecord {
        id: f.id.clone(),
        company_id: f.company_id.clone(),
        branch_id: Some(f.branch_id.clone()),
        department_id: f.department_id.clone(),
        version: f.version,
        title: f.title.clone(),
        status: f.status.clone(),
    };
    row.require_current(c).await?;
    if let Some(p) = purchase {
        PgCompanyPurchaseReader::new(holding.clone(), company.clone(), c, catalogue)
            .revalidate(&p, clock.now())
            .await?;
    }
    for p in &shadow_purchases {
        PgCompanyPurchaseReader::new(holding.clone(), company.clone(), c, catalogue)
            .revalidate(p, clock.now())
            .await?;
    }
    if clock.now().get() >= actor.expires_at() {
        return Err(missing().into());
    }
    Ok(FieldServiceSourceRecordResponse {
        data,
        context: context(actor)?,
    })
}
