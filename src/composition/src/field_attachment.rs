//! Field visit attachment authorization and Files-owned status/withdrawal composition.
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness,
};
use erpx_server_identity_access::public::effective_authority::{
    AuthorizedWorkContext, HoldingWorkSourcePreparation,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use serde::Serialize;
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_ATTACHMENT_CHANGED",
        "The original Service visit or selected role changed before its evidence committed.",
    )
}
fn missing() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ATTACHMENT_LINK_NOT_FOUND",
        "File link not found.",
    )
}
#[derive(Clone)]
pub(crate) struct FieldAttachmentSources;
impl HoldingWorkSourcePreparation for FieldAttachmentSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        lock_attachment_sources(c, false)
    }
}
pub(crate) fn lock_attachment_sources(
    c: &mut PgConnection,
    bind: bool,
) -> PgWorkFuture<'_, (), BusinessError> {
    Box::pin(async move {
        erpx_server_commercial::public::purchases::lock_commercial_authority_sources(c).await?;
        erpx_server_finance::public::field_service_workspace::lock_service_workspace_customers(c)
            .await?;
        if bind {
            // The Field update fires the retained Support projection trigger.
            // Acquire its owner's write-capable lock before the Field lock;
            // competing binds must not hold Support SHARE while upgrading it.
            erpx_server_support::public::field_service::lock_service_ticket_sources(c).await?;
        } else {
            erpx_server_support::public::field_service_workspace::lock_service_workspace_tickets(c).await?;
        }
        erpx_server_assets::public::field_service_workspace::lock_service_workspace_assets(c)
            .await?;
        erpx_server_people_payroll::public::field_service_workspace::lock_service_workspace_people(
            c,
        )
        .await?;
        erpx_server_identity_access::public::field_service_workspace::lock_service_workspace_authority(c).await?;
        if bind {
            erpx_server_field_service::public::file_source::lock_field_file_bind_sources(c).await?;
        } else {
            erpx_server_field_service::public::file_source::lock_field_file_sources(c).await?;
        }
        erpx_server_files::public::field_visit_link::lock_field_visit_link_sources(c).await?;
        Ok(())
    })
}
pub(crate) struct AttachmentContext<'a, C> {
    pub actor: &'a AuthorizedWorkContext,
    pub holding: &'a HoldingId,
    pub company: &'a CompanyId,
    pub catalogue: &'a CommercialCatalogue,
    pub clock: &'a C,
}
pub(crate) struct Proof {
 pub(crate) field:erpx_server_field_service::public::file_source::CurrentFieldFileSource,
 customers:erpx_server_finance::public::field_service_workspace::CurrentServiceWorkspaceCustomers,
 tickets:erpx_server_support::public::field_service_workspace::CurrentServiceWorkspaceTickets,
 assets:erpx_server_assets::public::field_service_workspace::CurrentServiceWorkspaceAssets,
 people:erpx_server_people_payroll::public::field_service_workspace::CurrentServiceWorkspacePeople,
 authority:erpx_server_identity_access::public::field_service_workspace::CurrentServiceWorkspaceAuthority,
 purchases:Vec<PurchaseWitness>,
}
impl Proof {
    pub(crate) async fn current<C: Clock>(
        &self,
        c: &mut PgConnection,
        a: &AuthorizedWorkContext,
        h: &HoldingId,
        company: &CompanyId,
        catalogue: &CommercialCatalogue,
        clock: &C,
    ) -> Result<(), Error> {
        self.field.require_current(c, a).await?;
        self.customers.require_current(c).await?;
        self.tickets.require_current(c).await?;
        self.assets.require_current(c).await?;
        self.people.require_current(c).await?;
        self.authority.require_current(c, clock.now()).await?;
        for p in &self.purchases {
            PgCompanyPurchaseReader::new(h.clone(), company.clone(), c, catalogue)
                .revalidate(p, clock.now())
                .await?;
        }
        if clock.now().get() >= a.expires_at() {
            return Err(unavailable().into());
        }
        Ok(())
    }
    pub(crate) async fn current_file_bind<C: Clock>(
        &self,
        c: &mut PgConnection,
        a: &AuthorizedWorkContext,
        h: &HoldingId,
        company: &CompanyId,
        catalogue: &CommercialCatalogue,
        clock: &C,
    ) -> Result<(), Error> {
        self.field.require_current(c, a).await?;
        self.customers.require_current(c).await?;
        self.tickets.require_field_file_current(c).await?;
        self.assets.require_current(c).await?;
        self.people.require_current(c).await?;
        self.authority.require_current(c, clock.now()).await?;
        for p in &self.purchases {
            PgCompanyPurchaseReader::new(h.clone(), company.clone(), c, catalogue)
                .revalidate(p, clock.now())
                .await?;
        }
        if clock.now().get() >= a.expires_at() {
            return Err(unavailable().into());
        }
        Ok(())
    }
}
pub(crate) async fn proof<C: Clock>(
    c: &mut PgConnection,
    ctx: &AttachmentContext<'_, C>,
    id: &str,
    write: bool,
) -> Result<Proof, Error> {
    let AttachmentContext {
        actor: a,
        holding: h,
        company,
        catalogue,
        clock,
    } = *ctx;
    use erpx_server_identity_access::public::field_service_workspace::{
        ServiceWorkspaceAssignmentQuery, ServiceWorkspaceAuthorityRequest,
        read_service_workspace_authority,
    };
    use erpx_server_support::public::field_service_workspace::{
        ServiceWorkspaceTicketScope, read_service_workspace_tickets,
    };
    let field =
        erpx_server_field_service::public::file_source::read_field_file_source(c, a, id).await?;
    let f = field.facts();
    if write && matches!(f.state.as_str(), "billed" | "cancelled") {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SERVICE_ATTACHMENT_HISTORY_ONLY",
            "This completed Service source retains its original private evidence as history.",
        )
        .into());
    }
    let customer =
        erpx_server_finance::public::field_service_workspace::read_service_workspace_customers(
            c,
            company,
            false,
            std::slice::from_ref(&f.customer_id),
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
    let tickets = read_service_workspace_tickets(
        c,
        ServiceWorkspaceTicketScope {
            company_id: company.clone(),
            branch: None,
            department: None,
            assignment_kind: "holding".into(),
            assignment_node: None,
            include_picker: false,
            ids: vec![f.ticket_id.clone()],
        },
    )
    .await?;
    if !customer
        .data()
        .display
        .iter()
        .any(|v| v.id == f.customer_id && v.kind == "customer")
        || !tickets.data().display.iter().any(|t| {
            t.id == f.ticket_id
                && t.customer_id == f.customer_id
                && t.branch_id == f.branch_id
                && t.department_id == f.department_id
        })
        || f.asset_id.as_ref().is_some_and(|id| {
            !assets
                .data()
                .display
                .iter()
                .any(|v| &v.id == id && v.branch_id == f.branch_id)
        })
    {
        return Err(missing().into());
    }
    let people =
        erpx_server_people_payroll::public::field_service_workspace::read_service_workspace_people(
            c,
            company,
            vec![f.technician_id.clone()],
            false,
        )
        .await?;
    let authority = read_service_workspace_authority(
        c,
        ServiceWorkspaceAuthorityRequest {
            company: company.clone(),
            actor: a.user_id().as_str().into(),
            assignments: vec![ServiceWorkspaceAssignmentQuery {
                user_id: f.technician_id.clone(),
                branch_id: f.branch_id.clone(),
                department_id: f.department_id.clone(),
            }],
            same_people: vec![],
        },
        clock.now(),
    )
    .await?;
    let _technician_eligible = people
        .data()
        .iter()
        .any(|p| p.user_id == f.technician_id && p.eligible)
        && authority
            .data()
            .assignments
            .iter()
            .any(|x| x.user_id == f.technician_id && x.write);
    let mut purchases = Vec::new();
    let purpose = if write {
        CapabilityPurpose::SettleExisting
    } else {
        CapabilityPurpose::History
    };
    let mut caps = vec!["service.visits"];
    if f.asset_id.is_some() || f.agreement_asset_id.is_some() {
        caps.push("service.parts-assets")
    };
    for cap in caps {
        let result = PgCompanyPurchaseReader::new(h.clone(), company.clone(), c, catalogue)
            .inspect(
                &PurchaseRequest {
                    capabilities: vec![cap.into()],
                    purpose,
                    mode: PackageEnforcementMode::Enforced,
                },
                clock.now(),
            )
            .await?;
        result.require_allowed()?;
        purchases.push(result.witness);
    }
    Ok(Proof {
        field,
        customers: customer,
        tickets,
        assets,
        people,
        authority,
        purchases,
    })
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FieldAttachmentWithdrawInput {
    pub reason: String,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldAttachmentScan {
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    pub attempts: i64,
    pub last_result: Option<String>,
    pub last_error: Option<String>,
    pub signature: Option<String>,
    #[cfg_attr(feature = "contracts", ts(type = "number | null"))]
    pub definition_updated_at: Option<i64>,
    #[cfg_attr(feature = "contracts", ts(type = "number | null"))]
    pub scanned_at: Option<i64>,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldAttachmentStatus {
    pub link_id: String,
    pub set_id: String,
    pub source_id: Option<String>,
    pub source_state: Option<String>,
    pub link_state: String,
    pub scan: FieldAttachmentScan,
    pub preview_available: bool,
    pub bindable: bool,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    pub expires_at: i64,
    #[cfg_attr(feature = "contracts", ts(type = "number | null"))]
    pub bound_at: Option<i64>,
    #[cfg_attr(feature = "contracts", ts(type = "number | null"))]
    pub withdrawn_at: Option<i64>,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FieldAttachmentStatusResponse {
    pub data: FieldAttachmentStatus,
    pub context: crate::field_service_reservation_read::FieldServiceRenderedContext,
}
pub(crate) async fn status<C: Clock>(
    c: &mut PgConnection,
    ctx: &AttachmentContext<'_, C>,
    id: &str,
    read_only: bool,
) -> Result<FieldAttachmentStatusResponse, Error> {
    let AttachmentContext {
        actor: a,
        holding: h,
        company,
        catalogue,
        clock,
    } = *ctx;
    let link =
        erpx_server_files::public::field_visit_link::read_field_visit_link(c, company, id).await?;
    let s = link.source();
    if !s.source_valid {
        return Err(missing().into());
    }
    if read_only && (s.state != "bound" || s.source_state.as_deref() != Some("available")) {
        return Err(missing().into());
    }
    if s.state == "draft" && s.created_by != a.user_id().as_str() {
        return Err(missing().into());
    }
    let evidence = proof(c, ctx, &s.target_id, false).await?;
    let scan = erpx_server_files::public::field_visit_link::read_field_visit_scan(
        c,
        s.attachment_id.as_deref(),
    )
    .await?;
    let data = FieldAttachmentStatus {
        link_id: s.id.clone(),
        set_id: s.set_id.clone(),
        source_id: s.attachment_id.clone(),
        source_state: s.source_state.clone(),
        link_state: s.state.clone(),
        scan: FieldAttachmentScan {
            attempts: scan.as_ref().map_or(0, |v| v.attempt),
            last_result: scan.as_ref().map(|v| v.state.clone()),
            last_error: scan.as_ref().and_then(|v| v.error_code.clone()),
            signature: scan.as_ref().and_then(|v| v.scanner_signature.clone()),
            definition_updated_at: scan.as_ref().and_then(|v| v.scanner_definition_at),
            scanned_at: scan.as_ref().and_then(|v| v.completed_at),
        },
        preview_available: s.preview_available,
        bindable: s.source_state.as_deref() == Some("available")
            && s.state == "draft"
            && evidence.field.facts().version == s.expected_target_version,
        expires_at: s.expires_at,
        bound_at: s.bound_at,
        withdrawn_at: s.withdrawn_at,
    };
    let reply = FieldAttachmentStatusResponse {
        data,
        context: crate::field_service_reservation_read::context(a)?,
    };
    let value = serde_json::to_value(&reply).map_err(|_| unavailable())?;
    if !crate::field_service_workspace::safe_json(&value) {
        return Err(unavailable().into());
    }
    serde_json::to_writer(
        crate::field_service_reservation_read::ReplyBudget(0),
        &value,
    )
    .map_err(|_| unavailable())?;
    link.require_current(c).await?;
    evidence.current(c, a, h, company, catalogue, clock).await?;
    erpx_server_files::public::field_visit_link::write_status_access(
        c,
        company,
        s,
        a.user_id().as_str(),
        clock.now().get(),
    )
    .await?;
    link.require_current(c).await?;
    if erpx_server_files::public::field_visit_link::read_field_visit_scan(
        c,
        s.attachment_id.as_deref(),
    )
    .await?
        != scan
    {
        return Err(unavailable().into());
    }
    evidence.current(c, a, h, company, catalogue, clock).await?;
    Ok(reply)
}
pub(crate) async fn withdraw<C: Clock>(
    c: &mut PgConnection,
    ctx: &AttachmentContext<'_, C>,
    id: &str,
    key: &str,
    reason: &str,
) -> Result<(), Error> {
    let AttachmentContext {
        actor: a,
        holding: h,
        company,
        catalogue,
        clock,
    } = *ctx;
    let link =
        erpx_server_files::public::field_visit_link::read_field_visit_link(c, company, id).await?;
    let s = link.source();
    erpx_server_files::public::field_visit_link::check_withdrawal_actor(
        &s.state,
        &s.created_by,
        a.user_id().as_str(),
    )?;
    let evidence = proof(c, ctx, &s.target_id, true).await?;
    evidence.current(c, a, h, company, catalogue, clock).await?;
    link.withdraw(c, a.user_id().as_str(), reason, key, clock.now().get())
        .await?;
    evidence.current(c, a, h, company, catalogue, clock).await?;
    Ok(())
}
pub(crate) fn normalize_reason(bytes: &[u8]) -> Result<String, BusinessError> {
    use erpx_server_workflow::public::commands::CommandJson;
    let text = String::from_utf8_lossy(bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let json = CommandJson::parse(text).map_err(|_| {
        BusinessError::new(
            FailureKind::InvalidInput,
            "VALIDATION_FAILED",
            "A withdrawal reason is required.",
        )
    })?;
    let mut units = json
        .property("reason")
        .ok()
        .flatten()
        .and_then(|v| v.string_utf16_units().ok().flatten())
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::InvalidInput,
                "VALIDATION_FAILED",
                "A withdrawal reason is required.",
            )
        })?;
    let whitespace = |u: &u16| matches!(*u,0x0009..=0x000d|0x0020|0x00a0|0x1680|0x2000..=0x200a|0x2028|0x2029|0x202f|0x205f|0x3000|0xfeff);
    let start = units
        .iter()
        .position(|u| !whitespace(u))
        .unwrap_or(units.len());
    let end = units
        .iter()
        .rposition(|u| !whitespace(u))
        .map_or(start, |x| x + 1);
    units = units[start..end].to_vec();
    if !(3..=1000).contains(&units.len()) {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "VALIDATION_FAILED",
            "A withdrawal reason is required.",
        ));
    }
    Ok(String::from_utf16_lossy(&units))
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldAttachmentBindInput {
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    pub expected_target_version: i64,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldAttachmentManifestItem {
    pub link_id: String,
    pub source_id: String,
    pub purpose_key: String,
    pub checksum_sha256: String,
    pub media_type: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    pub bytes: i64,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldAttachmentBindResult {
    pub set_id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    pub version: i64,
    pub manifest: Vec<FieldAttachmentManifestItem>,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FieldAttachmentBindResponse {
    pub data: FieldAttachmentBindResult,
    pub context: crate::field_service_reservation_read::FieldServiceRenderedContext,
}
