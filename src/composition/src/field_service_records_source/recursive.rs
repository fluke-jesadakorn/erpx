//! Support's schema permits one typed POS/PMS/Asset origin. Retain that actual
//! owner snapshot and current purchase in the same selected transaction. No
//! guessed recursion through Field navigation links and no HTTP source guards.
use super::*;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, PurchaseRequest, PurchaseWitness,
};
#[allow(clippy::large_enum_variant)]
enum Leaf {
    Asset(erpx_server_assets::public::record_source::CurrentRecordSource),
    Pos(erpx_server_pos::public::record_source::CurrentRecordSource),
    Pms {
        source: erpx_server_pms::public::record_source::CurrentRecordSource,
        branch: erpx_server_organization::public::pms_source_branch::CurrentPmsSourceBranch,
    },
}
pub(crate) struct PreparedLeafSource {
    source: Leaf,
    purchase: PurchaseWitness,
    record: erpx_server_records::public::source_record::SourceRecord,
}
impl PreparedLeafSource {
    pub(crate) fn record(&self) -> &erpx_server_records::public::source_record::SourceRecord {
        &self.record
    }
    pub(crate) async fn require_current<C: Clock>(
        &self,
        c: &mut PgConnection,
        ctx: &SourceRecordContext<'_, C>,
    ) -> Result<(), Error> {
        match &self.source {
            Leaf::Asset(v) => v.require_current(c).await?,
            Leaf::Pos(v) => v.require_current(c).await?,
            Leaf::Pms { source, branch } => {
                source.require_current(c).await?;
                branch.require_current(c).await?;
            }
        }
        ctx.purchase_reader(c)
            .revalidate(&self.purchase, ctx.clock.now())
            .await?;
        Ok(())
    }
}
fn missing() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "SOURCE_RECORD_NOT_FOUND",
        "Source record is unavailable in this work scope.",
    )
}
fn scope<C>(ctx: &SourceRecordContext<'_, C>, row: &SourceRecord) -> Result<(), BusinessError> {
    let s = ctx.actor.selection().workspace();
    let a = ctx.actor.selection().assignment();
    let node = a.scope_node_id.as_ref().map(|v| v.as_str());
    let visible = match a.scope_kind {
        erpx_server_identity_access::public::work_context::WorkScopeKind::Holding => true,
        erpx_server_identity_access::public::work_context::WorkScopeKind::Branch => {
            node.is_some() && node == row.branch_id.as_deref()
        }
        erpx_server_identity_access::public::work_context::WorkScopeKind::Department => {
            node.is_some() && node == row.department_id.as_deref()
        }
    };
    if !visible
        || row.company_id != ctx.company.as_str()
        || s.branch_id
            .as_ref()
            .is_some_and(|v| Some(v.as_str()) != row.branch_id.as_deref())
        || s.department_id
            .as_ref()
            .is_some_and(|v| Some(v.as_str()) != row.department_id.as_deref())
    {
        return Err(missing());
    }
    Ok(())
}
pub(super) async fn prepare<C: Clock>(
    c: &mut PgConnection,
    ctx: &SourceRecordContext<'_, C>,
    reference: &SourceReference,
) -> Result<PreparedLeafSource, Error> {
    erpx_server_records::public::source_record::validate_reference(reference)?;
    let actor = ctx.actor;
    let (source, record, capability) = match reference.kind {
        SourceKind::Asset => {
            actor.require_permission("records.read")?;
            if !actor.modules().iter().any(|v| v == "assets") {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "MODULE_FORBIDDEN",
                    "The Assets workspace is unavailable.",
                )
                .into());
            }
            let v = erpx_server_assets::public::record_source::read_record_source(
                c,
                ctx.company,
                &reference.id,
            )
            .await?
            .ok_or_else(missing)?;
            let record = v.data().clone();
            (Leaf::Asset(v), record, "assets.register")
        }
        SourceKind::PosSale => {
            actor.require_permission("pos.read")?;
            if actor.selection().workspace().department_id.is_some() {
                return Err(missing().into());
            }
            let v = erpx_server_pos::public::record_source::read_record_source(
                c,
                ctx.company,
                &reference.id,
            )
            .await?
            .ok_or_else(missing)?;
            let record = v.data().clone();
            (Leaf::Pos(v), record, "pos.sales")
        }
        SourceKind::PmsRecord => {
            actor.require_permission("pms.read")?;
            if actor.selection().workspace().department_id.is_some() {
                return Err(missing().into());
            }
            let v = erpx_server_pms::public::record_source::read_record_source(
                c,
                ctx.company,
                &reference.id,
            )
            .await?
            .ok_or_else(missing)?;
            v.require_reader(actor.user_id().as_str(), |p| actor.has_permission(p))?;
            let record = v.data().clone();
            let branch =
                erpx_server_organization::public::pms_source_branch::read_pms_source_branch(
                    c,
                    ctx.company,
                    record.branch_id.as_deref().ok_or_else(missing)?,
                )
                .await?
                .ok_or_else(missing)?;
            (Leaf::Pms { source: v, branch }, record, "pms.front-desk")
        }
        _ => {
            return Err(BusinessError::invalid(
                "SOURCE_RECORD_INVALID",
                "Unsupported Support origin.",
            )
            .into());
        }
    };
    scope(ctx, &record)?;
    erpx_server_records::public::source_record::require_matching_source(
        reference,
        ctx.company.as_str(),
        &record,
    )?;
    let purchase =
        ctx.purchase_reader(c)
            .inspect(
                &PurchaseRequest {
                    capabilities: vec![capability.into()],
                    purpose: CapabilityPurpose::History,
                    mode: PackageEnforcementMode::Enforced,
                },
                ctx.clock.now(),
            )
            .await?;
    purchase.require_allowed()?;
    Ok(PreparedLeafSource {
        source,
        purchase: purchase.witness,
        record,
    })
}
