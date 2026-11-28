//! Scoped RFQ award comparison, after the original input normalization.
//! The caller must obtain the current RFQ, offer, demand and purchase order
//! facts in the same protected unit and revalidate their sources.
use erpx_platform_shared_server::public::{BusinessError, FailureKind};

#[derive(Clone, Debug)]
pub struct RfqAward {
    pub status: String,
    pub request_id: Option<String>,
    pub selected_offer_id: Option<String>,
    pub order_id: Option<String>,
    pub warehouse_id: String,
    pub needed_date: String,
    pub demands: Vec<RfqAwardDemand>,
    pub offers: Vec<RfqAwardOffer>,
    pub offer_lines: Vec<RfqAwardOfferLine>,
}
#[derive(Clone, Debug)]
pub struct RfqAwardDemand {
    pub id: String,
    pub product_id: String,
    pub quantity_milli: i64,
}
#[derive(Clone, Debug)]
pub struct RfqAwardOffer {
    pub id: String,
    pub supplier_id: String,
    pub valid_until: String,
}
#[derive(Clone, Debug)]
pub struct RfqAwardOfferLine {
    pub offer_id: String,
    pub rfq_line_id: String,
    pub price_minor: i64,
    pub tax_bps: i64,
}
#[derive(Clone, Debug)]
pub struct RfqPurchaseOrder {
    pub kind: String,
    pub request_id: Option<String>,
    pub party_id: String,
    pub warehouse_id: String,
    pub due_date: String,
    pub order_date: String,
    pub lines: Vec<RfqPurchaseOrderLine>,
}
#[derive(Clone, Debug)]
pub struct RfqPurchaseOrderLine {
    pub product_id: String,
    /// Parsed with the original Commerce quantity rules.
    pub quantity_milli: i64,
    /// Parsed with the original Finance decimal rules.
    pub price_minor: i64,
    pub tax_bps: i64,
}
fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

/// Validate only comparison and error precedence from `assertRfqOrder`.
/// This result conveys no authority or evidence that the sources are current.
pub fn assert_rfq_order<'a>(
    award: &'a RfqAward,
    input: &RfqPurchaseOrder,
    order_id: &str,
    editing: bool,
    today: &str,
) -> Result<&'a RfqAwardOffer, BusinessError> {
    let selected = award
        .offers
        .iter()
        .find(|offer| Some(&offer.id) == award.selected_offer_id.as_ref());
    if input.kind != "purchase"
        || selected.is_none()
        || (!editing && award.status != "approved")
        || (editing && award.order_id.as_deref() != Some(order_id))
    {
        return Err(conflict(
            "RFQ_AWARD_REQUIRED",
            "An independently approved RFQ award is required.",
        ));
    }
    let selected = selected.expect("checked above");
    if !editing && award.request_id.is_some() && input.request_id != award.request_id {
        return Err(conflict(
            "RFQ_REQUEST_LINK_REQUIRED",
            "The awarded PO must also hand off its approved requisition.",
        ));
    }
    let matches = input.lines.iter().all(|line| {
        let Some(demand) = award
            .demands
            .iter()
            .find(|d| d.product_id == line.product_id)
        else {
            return false;
        };
        let Some(price) = award
            .offer_lines
            .iter()
            .find(|p| p.offer_id == selected.id && p.rfq_line_id == demand.id)
        else {
            return false;
        };
        line.quantity_milli == demand.quantity_milli
            && line.price_minor == price.price_minor
            && line.tax_bps == price.tax_bps
    });
    if input.party_id != selected.supplier_id
        || input.warehouse_id != award.warehouse_id
        || input.due_date != award.needed_date
        || input.lines.len() != award.demands.len()
        || !matches
    {
        return Err(conflict(
            "RFQ_AWARD_MISMATCH",
            "Supplier, warehouse, required date, quantities, prices and taxes must match the approved award.",
        ));
    }
    if !editing
        && (selected.valid_until.as_str() < today || input.order_date > selected.valid_until)
    {
        return Err(conflict(
            "RFQ_QUOTE_EXPIRED",
            "Obtain a new quote and independent award before ordering.",
        ));
    }
    Ok(selected)
}
