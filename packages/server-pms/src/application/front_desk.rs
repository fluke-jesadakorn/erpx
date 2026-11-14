//! Native PMS front-desk queue read use cases. The repository owns only PMS
//! tables; current package entitlement arrives through the public Commercial
//! boundary supplied by composition. This is a read projection, never a
//! Command receipt.
use crate::application::room_board::{PmsBoardEntitlementPort, PmsBoardFuture, empty_workspace};
use crate::domain::front_desk::{
    FRONT_DESK_KIND, FRONT_DESK_SECTION, PmsFrontDeskData, PmsFrontDeskSelection, read_invalid,
    validate_front_desk_data,
};
use crate::domain::room_board::{
    MAX_PROPERTIES, PmsBoardActor, PmsBoardSetup, PmsBoardVisibility, PmsListMeta, PmsPropertyRow,
    PmsSnapshot, PmsWorkspace, booking_readiness, default_date, not_found, readiness_from_facts,
    redact_config, validate_properties, visible_records,
};
use erpx_server_shared::public::BusinessError;

pub trait PmsFrontDeskRepository: Send {
    type Error: From<BusinessError> + Send;
    fn properties<'a>(
        &'a mut self,
        company_id: &'a str,
        branch_id: Option<&'a str>,
    ) -> PmsBoardFuture<'a, Vec<PmsPropertyRow>, Self::Error>;
    fn front_desk<'a>(
        &'a mut self,
        property_id: &'a str,
        business_date: &'a str,
        selection: &'a PmsFrontDeskSelection,
        visibility: PmsBoardVisibility,
    ) -> PmsBoardFuture<'a, PmsFrontDeskData, Self::Error>;
}

pub async fn pms_front_desk<R, E>(
    actor: &PmsBoardActor,
    selection: &PmsFrontDeskSelection,
    repository: &mut R,
    entitlement: &mut E,
    now: i64,
) -> Result<PmsWorkspace, R::Error>
where
    R: PmsFrontDeskRepository,
    E: PmsBoardEntitlementPort,
    R::Error: From<E::Error>,
{
    actor.require_front_desk(now)?;
    let mut normalized = selection.clone();
    normalized.normalize();
    normalized.validate()?;
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
    let selected = match normalized.property_id.as_deref() {
        Some(id) => properties
            .iter()
            .find(|row| row.id == id)
            .cloned()
            .ok_or_else(not_found)?,
        None => match properties.first() {
            Some(row) => row.clone(),
            None => return Ok(empty_workspace(actor, properties, Vec::new(), now)),
        },
    };
    let data = validate_front_desk_data(
        actor,
        repository
            .front_desk(
                &selected.id,
                &selected.business_date,
                &normalized,
                actor.visibility(),
            )
            .await?,
    )?;
    if data.property.id != selected.id
        || data.property.company_id != actor.company_id
        || data.property.business_date != selected.business_date
    {
        return Err(read_invalid().into());
    }
    let mut property = data.property.view()?;
    let readiness_checks = readiness_from_facts(
        &property,
        PmsBoardSetup {
            has_room_types: data.records.iter().any(|row| row.kind == "room_type"),
            has_active_rooms: data.records.iter().any(|row| {
                row.kind == "room"
                    && row.data.get("active").and_then(serde_json::Value::as_bool) == Some(true)
            }),
            has_active_rates: data.records.iter().any(|row| {
                row.kind == "rate"
                    && row.data.get("active").and_then(serde_json::Value::as_bool) == Some(true)
            }),
        },
    );
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
    let selected_id = normalized
        .selected_id
        .as_ref()
        .filter(|id| visible.contains(id.as_str()))
        .cloned();
    let documents = if actor.can_financial() {
        let accounts: std::collections::BTreeSet<&str> = records
            .iter()
            .filter(|row| row.kind == "guest_account")
            .map(|row| row.id.as_str())
            .collect();
        data.documents
            .into_iter()
            .filter(|document| {
                document
                    .get("guest_account_id")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|id| accounts.contains(id))
            })
            .collect()
    } else {
        Vec::new()
    };
    let list_meta = PmsListMeta {
        room_summary: None,
        section: FRONT_DESK_SECTION.to_owned(),
        kind: FRONT_DESK_KIND.to_owned(),
        ids,
        from: data.from,
        to: data.to,
        page: normalized.page,
        q: normalized.q.clone(),
        status: normalized.status.clone(),
        building: String::new(),
        floor: String::new(),
        type_id: String::new(),
        has_next: data.has_next,
        selected_id,
    };
    let front_ready = property.state == "open" && readiness_checks.is_empty();
    Ok(PmsWorkspace {
        properties: properties.iter().map(PmsPropertyRow::summary).collect(),
        snapshot: Some(PmsSnapshot {
            property: property.clone(),
            records,
            list_meta: Some(list_meta),
        }),
        default_date: default_date(now),
        readiness: readiness_checks,
        front_ready: Some(front_ready),
        can_review_booking_media: actor.can_review_booking_media(),
        booking_readiness: booking_readiness(&property),
        can_manage_setup: actor.can_manage_setup(),
        permissions: actor.permissions(),
        available_payment_methods: vec!["cash".to_owned()],
        user_id: actor.user_id.clone(),
        branches: Vec::new(),
        staff: Vec::new(),
        accounts: Vec::new(),
        charts: Vec::new(),
        parties: Vec::new(),
        products: Vec::new(),
        warehouses: Vec::new(),
        documents,
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
