//! Native PMS property/room board read use cases. The repository owns only PMS
//! tables; current staff labels arrive through the public People port supplied
//! by composition. This is a read projection, never a Command receipt.
use crate::domain::room_board::{
    MAX_PROPERTIES, PmsBoardActor, PmsBoardData, PmsBoardEntitlement, PmsBoardSelection,
    PmsBoardVisibility, PmsBookingReadiness, PmsListMeta, PmsPropertyRow, PmsSnapshot, PmsStaff,
    PmsWorkspace, booking_readiness, default_date, not_found, read_invalid, readiness_from_facts,
    redact_config, staff, validate_board_data, validate_properties, visible_records,
};
use erpx_server_shared::public::BusinessError;
use std::future::Future;
use std::pin::Pin;

pub type PmsBoardFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait PmsBoardRepository: Send {
    type Error: From<BusinessError> + Send;
    fn properties<'a>(
        &'a mut self,
        company_id: &'a str,
        branch_id: Option<&'a str>,
    ) -> PmsBoardFuture<'a, Vec<PmsPropertyRow>, Self::Error>;
    fn board<'a>(
        &'a mut self,
        property_id: &'a str,
        business_date: &'a str,
        selection: &'a PmsBoardSelection,
        visibility: PmsBoardVisibility,
    ) -> PmsBoardFuture<'a, PmsBoardData, Self::Error>;
}

/// Identity/People public recipient reference. Selection is current; this
/// projection grants no assignment or command authority.
pub trait PmsStaffDirectory {
    type Error: Send;
    fn staff<'a>(
        &'a mut self,
        company_id: &'a str,
        branch_id: Option<&'a str>,
    ) -> PmsBoardFuture<'a, Vec<PmsStaff>, Self::Error>;
}

/// Commercial public package boundary. `enforced=false` is the retained
/// disabled/pilot policy; when enforced, the current `pms.front-desk` history
/// entitlement is required exactly as the TypeScript workspace reader does.
pub trait PmsBoardEntitlementPort {
    type Error: Send;
    fn entitlement<'a>(
        &'a mut self,
        company_id: &'a str,
    ) -> PmsBoardFuture<'a, PmsBoardEntitlement, Self::Error>;
}

pub async fn pms_room_board<R, S, E>(
    actor: &PmsBoardActor,
    selection: &PmsBoardSelection,
    repository: &mut R,
    staff_directory: &mut S,
    entitlement: &mut E,
    now: i64,
) -> Result<PmsWorkspace, R::Error>
where
    R: PmsBoardRepository,
    S: PmsStaffDirectory,
    E: PmsBoardEntitlementPort,
    R::Error: From<S::Error> + From<E::Error>,
{
    actor.require_board(now)?;
    actor.require_section(selection.section)?;
    selection.validate()?;
    entitlement
        .entitlement(&actor.company_id)
        .await
        .map_err(R::Error::from)?
        .require_history()?;
    let properties = validate_properties(
        actor,
        repository
            .properties(&actor.company_id, actor.branch_id.as_deref())
            .await?,
    )?;
    if properties.len() > MAX_PROPERTIES {
        return Err(read_invalid().into());
    }
    let selected = match selection.property_id.as_deref() {
        Some(id) => properties
            .iter()
            .find(|row| row.id == id)
            .cloned()
            .ok_or_else(not_found)?,
        None => match properties.first() {
            Some(row) => row.clone(),
            None => {
                let staff_rows = staff(
                    staff_directory
                        .staff(&actor.company_id, actor.branch_id.as_deref())
                        .await
                        .map_err(R::Error::from)?,
                )?;
                return Ok(empty_workspace(actor, properties, staff_rows, now));
            }
        },
    };
    let data = validate_board_data(
        actor,
        repository
            .board(
                &selected.id,
                &selected.business_date,
                selection,
                actor.visibility(),
            )
            .await?,
    )?;
    if data.property.id != selected.id
        || data.property.company_id != actor.company_id
        || data.business_date_mismatch(&selected.business_date)
    {
        return Err(read_invalid().into());
    }
    let mut property = data.property.view()?;
    let readiness_checks = readiness_from_facts(&property, data.setup);
    redact_config(actor, &mut property);
    let records = visible_records(actor, data.records)?;
    let visible: std::collections::BTreeSet<&str> =
        records.iter().map(|row| row.id.as_str()).collect();
    let ids: Vec<String> = data
        .ids
        .iter()
        .filter(|id| visible.contains(id.as_str()))
        .cloned()
        .collect();
    let selected_id = selection
        .selected_id
        .as_ref()
        .filter(|id| visible.contains(id.as_str()))
        .cloned();
    let list_meta = PmsListMeta {
        room_summary: Some(data.room_summary),
        section: selection.section.as_str().to_owned(),
        kind: "room".to_owned(),
        ids,
        from: data.from,
        to: data.to,
        page: selection.page,
        q: selection.q.clone(),
        status: selection.status.clone(),
        building: selection.building.clone(),
        floor: selection.floor.clone(),
        type_id: selection.type_id.clone().unwrap_or_default(),
        has_next: data.has_next,
        selected_id,
    };
    let staff_rows = staff(
        staff_directory
            .staff(&actor.company_id, actor.branch_id.as_deref())
            .await
            .map_err(R::Error::from)?,
    )?;
    Ok(PmsWorkspace {
        properties: properties.iter().map(PmsPropertyRow::summary).collect(),
        snapshot: Some(PmsSnapshot {
            property: property.clone(),
            records,
            list_meta: Some(list_meta),
        }),
        default_date: default_date(now),
        readiness: readiness_checks,
        front_ready: None,
        can_review_booking_media: actor.can_review_booking_media(),
        booking_readiness: booking_readiness(&property),
        can_manage_setup: actor.can_manage_setup(),
        permissions: actor.permissions(),
        available_payment_methods: vec!["cash".to_owned()],
        user_id: actor.user_id.clone(),
        branches: Vec::new(),
        staff: staff_rows,
        accounts: Vec::new(),
        charts: Vec::new(),
        parties: Vec::new(),
        products: Vec::new(),
        warehouses: Vec::new(),
        documents: Vec::new(),
        transfer_requests: Vec::new(),
        charge_corrections: Vec::new(),
        stripe_connection_ready: false,
        stripe_payments: Vec::new(),
        counter_refunds: Vec::new(),
        booking_refunds: Vec::new(),
        booking_delivery: Vec::new(),
        ledger: None,
        control_balances: None,
        day_controls: None,
    })
}

impl PmsBoardData {
    fn business_date_mismatch(&self, expected: &str) -> bool {
        self.property.business_date != expected
    }
}

fn empty_workspace(
    actor: &PmsBoardActor,
    properties: Vec<PmsPropertyRow>,
    staff_rows: Vec<PmsStaff>,
    now: i64,
) -> PmsWorkspace {
    PmsWorkspace {
        properties: properties.iter().map(PmsPropertyRow::summary).collect(),
        snapshot: None,
        default_date: default_date(now),
        readiness: vec!["Add a hotel property".to_owned()],
        front_ready: Some(false),
        can_review_booking_media: actor.can_review_booking_media(),
        booking_readiness: PmsBookingReadiness::default(),
        can_manage_setup: actor.can_manage_setup(),
        permissions: actor.permissions(),
        available_payment_methods: vec!["cash".to_owned()],
        user_id: actor.user_id.clone(),
        branches: Vec::new(),
        staff: staff_rows,
        accounts: Vec::new(),
        charts: Vec::new(),
        parties: Vec::new(),
        products: Vec::new(),
        warehouses: Vec::new(),
        documents: Vec::new(),
        transfer_requests: Vec::new(),
        charge_corrections: Vec::new(),
        stripe_connection_ready: false,
        stripe_payments: Vec::new(),
        counter_refunds: Vec::new(),
        booking_refunds: Vec::new(),
        booking_delivery: Vec::new(),
        ledger: None,
        control_balances: None,
        day_controls: None,
    }
}
