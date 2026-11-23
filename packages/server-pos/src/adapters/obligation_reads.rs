//! Native POS add-on/package obligation read kinds. These closed `SELECT`-only
//! owner statements are the runtime authority for the retained TypeScript
//! addon, core-package and PMS-package obligation readers. The bridge sends a
//! typed kind and positional Company/package values; this module selects the
//! SQL. There is no TypeScript mirror or fallback for these facts reads.
//!
//! The settlement-source projections returned by `settlement_sources` remain
//! owner-owned SQL, but they are consumed as SQL text by Finance, Inventory and
//! PMS readers (cross-owner), so those callers stay in TypeScript and are
//! documented at the adapter boundary until their own native ports exist.

use std::sync::LazyLock;

pub const ADDON_RESTAURANT: &str = "pos_addon_obligations_restaurant";
pub const ADDON_HOTEL: &str = "pos_addon_obligations_hotel";
pub const PACKAGE: &str = "pos_package_obligations";
pub const PMS_PACKAGE: &str = "pms_pos_package_obligations";

pub const KINDS: &[&str] = &[ADDON_RESTAURANT, ADDON_HOTEL, PACKAGE, PMS_PACKAGE];

pub const POS_RESTAURANT: &str = "pos-restaurant";
pub const POS_HOTEL_CHARGE: &str = "pos-hotel-charge";

pub fn supported(kind: &str) -> bool {
    KINDS.contains(&kind)
}

/// The closed PMS projection fragments inlined by the hotel addon, copied
/// byte-identically from `pmsPackageSettlementSources` in
/// `packages/server-pms/src/adapters/package-obligations.ts`.
pub const PMS_ROOM_CHARGES: &str =
    "SELECT p.company_id,s.property_id,s.source_key FROM pms_sources s JOIN pms_properties p ON p.id=s.property_id WHERE s.source_key LIKE 'pos:%'";
pub const PMS_PROPERTIES: &str = "SELECT id,company_id,branch_id,config_json FROM pms_properties";

/// All closed fragments for one addon package. `sales` carries the two
/// registered addon scopes, `events` feeds Stripe callback detection and the
/// remaining fragments are reused by the facts read and the settlement
/// projections. Package scope is fixed by the validated constructor; Company
/// scope is bound by the caller's transaction.
#[derive(Clone, Debug)]
pub struct Fragments {
    pub sales: String,
    pub stock_fulfillments: String,
    pub shifts: String,
    pub payments: String,
    pub refunds: String,
    pub refund_payments: String,
    pub events: String,
    pub posting_keys: String,
}

fn addon_fragments(package_id: &str) -> Option<Fragments> {
    let sales = match package_id {
        POS_RESTAURANT => concat!(
            "SELECT 'pos-restaurant'::text package_id,s.* FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id AND r.company_id=s.company_id\n",
            "  WHERE s.service_mode IN ('dine_in','takeaway') OR r.mode='restaurant' OR EXISTS(SELECT 1 FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id AND f.company_id=s.company_id WHERE l.sale_id=s.id AND f.kind='recipe')",
        )
        .to_owned(),
        POS_HOTEL_CHARGE => format!(
            concat!(
                "SELECT 'pos-hotel-charge'::text package_id,s.* FROM pos_sales s WHERE EXISTS(\n",
                "    SELECT 1 FROM ({room_charges}) source JOIN ({properties}) p ON p.id=source.property_id AND p.company_id=source.company_id\n",
                "    WHERE source.company_id=s.company_id AND p.branch_id=s.branch_id AND source.source_key='pos:'||s.id||':'||s.id)\n",
                "    OR EXISTS(SELECT 1 FROM pos_room_charge_reversal_requests request WHERE request.company_id=s.company_id AND request.sale_id=s.id)",
            ),
            room_charges = PMS_ROOM_CHARGES,
            properties = PMS_PROPERTIES,
        ),
        _ => return None,
    };
    let stock_fulfillments = format!(
        "SELECT DISTINCT s.package_id,f.company_id,f.id,f.version,f.state,f.kind,f.cost_minor,f.quantity_milli,f.currency,f.accounts_json FROM ({sales}) s JOIN pos_lines l ON l.sale_id=s.id JOIN pos_fulfillments f ON f.id=l.fulfillment_id AND f.company_id=s.company_id",
        sales = sales,
    );
    let shifts = format!(
        concat!(
            "SELECT DISTINCT 'pos-restaurant'::text package_id,s.* FROM pos_shifts s JOIN pos_registers r ON r.id=s.register_id AND r.company_id=s.company_id\n",
            "  WHERE r.mode='restaurant' OR EXISTS(SELECT 1 FROM ({sales}) sale WHERE sale.package_id='pos-restaurant' AND sale.company_id=s.company_id AND sale.shift_id=s.id)",
        ),
        sales = sales,
    );
    let payments = format!(
        "SELECT s.package_id,p.* FROM ({sales}) s JOIN pos_payments p ON p.sale_id=s.id AND p.company_id=s.company_id",
        sales = sales,
    );
    let refunds = format!(
        "SELECT s.package_id,r.* FROM ({sales}) s JOIN pos_refunds r ON r.sale_id=s.id AND r.company_id=s.company_id",
        sales = sales,
    );
    let refund_payments = format!(
        "SELECT r.package_id,r.company_id,r.sale_id,p.* FROM ({refunds}) r JOIN pos_refund_payments p ON p.refund_id=r.id",
        refunds = refunds,
    );
    let events = format!(
        concat!(
            "SELECT DISTINCT p.package_id,e.company_id,e.id,e.applied_at FROM pos_stripe_events e JOIN ({payments}) p\n",
            "    ON p.company_id=e.company_id AND p.stripe_account=e.account_id AND p.livemode=e.livemode\n",
            "    WHERE e.payload_json::jsonb#>>'{{metadata,pos_payment_id}}'=p.id\n",
            "  UNION SELECT DISTINCT rp.package_id,e.company_id,e.id,e.applied_at FROM pos_stripe_events e JOIN ({refund_payments}) rp ON rp.company_id=e.company_id\n",
            "    JOIN pos_payments p ON p.id=rp.payment_id AND p.company_id=rp.company_id AND p.stripe_account=e.account_id AND p.livemode=e.livemode\n",
            "    WHERE e.payload_json::jsonb#>>'{{metadata,pos_refund_payment_id}}'=rp.id",
        ),
        payments = payments,
        refund_payments = refund_payments,
    );
    let posting_keys = format!(
        concat!(
            "SELECT s.package_id,s.company_id,'sale:'||s.id source_key,s.id source_id,s.currency,s.accounts_json FROM ({sales}) s\n",
            "  UNION SELECT s.package_id,s.company_id,'allocate:'||s.id,s.id,s.currency,s.accounts_json FROM ({sales}) s\n",
            "  UNION SELECT s.package_id,s.company_id,'room-reversal:'||s.id,s.id,s.currency,s.accounts_json FROM ({sales}) s\n",
            "  UNION SELECT p.package_id,p.company_id,'receipt:'||p.id,s.id,s.currency,s.accounts_json FROM ({payments}) p JOIN ({sales}) s ON s.id=p.sale_id AND s.package_id=p.package_id AND s.company_id=p.company_id\n",
            "  UNION SELECT f.package_id,f.company_id,prefix||f.id,f.id,f.currency,f.accounts_json FROM ({stock_fulfillments}) f CROSS JOIN (VALUES ('consume:'),('deliver:'),('waste:')) source(prefix)\n",
            "  UNION SELECT r.package_id,r.company_id,'refund:'||r.id,r.id,s.currency,s.accounts_json FROM ({refunds}) r JOIN ({sales}) s ON s.id=r.sale_id AND s.package_id=r.package_id AND s.company_id=r.company_id\n",
            "  UNION SELECT r.package_id,r.company_id,'return:'||r.id||':'||l.id,r.id,s.currency,s.accounts_json FROM ({refunds}) r JOIN pos_lines l ON l.sale_id=r.sale_id JOIN ({sales}) s ON s.id=r.sale_id AND s.package_id=r.package_id AND s.company_id=r.company_id\n",
            "  UNION SELECT rp.package_id,rp.company_id,'refund-payment:'||rp.id,rp.refund_id,s.currency,s.accounts_json FROM ({refund_payments}) rp JOIN ({sales}) s ON s.id=rp.sale_id AND s.package_id=rp.package_id AND s.company_id=rp.company_id\n",
            "  -- Old cash/shift rows do not capture currency/account policy. Bind their\n",
            "  -- exact native source and memo; never reinterpret history using today's settings.\n",
            "  UNION SELECT s.package_id,s.company_id,'shift:'||s.id,s.id,NULL::text,NULL::text FROM ({shifts}) s\n",
            "  UNION SELECT s.package_id,s.company_id,'cash:'||m.id,s.id,NULL::text,NULL::text FROM ({shifts}) s JOIN pos_cash_movements m ON m.shift_id=s.id",
        ),
        sales = sales,
        payments = payments,
        stock_fulfillments = stock_fulfillments,
        refunds = refunds,
        refund_payments = refund_payments,
        shifts = shifts,
    );
    Some(Fragments { sales, stock_fulfillments, shifts, payments, refunds, refund_payments, events, posting_keys })
}

fn addon_facts(package_id: &str) -> Option<String> {
    let f = addon_fragments(package_id)?;
    Some(format!(
        concat!(
            "WITH scope AS (SELECT ?::text company_id,?::text package_id),\n",
            "  sales AS MATERIALIZED (SELECT source.* FROM ({sales}) source,scope WHERE source.company_id=scope.company_id AND source.package_id=scope.package_id),\n",
            "  fulfillments AS MATERIALIZED (SELECT source.* FROM ({stock_fulfillments}) source,scope WHERE source.company_id=scope.company_id AND source.package_id=scope.package_id),\n",
            "  payments AS MATERIALIZED (SELECT source.* FROM ({payments}) source,scope WHERE source.company_id=scope.company_id AND source.package_id=scope.package_id),\n",
            "  refunds AS MATERIALIZED (SELECT source.* FROM ({refunds}) source,scope WHERE source.company_id=scope.company_id AND source.package_id=scope.package_id),\n",
            "  payouts AS MATERIALIZED (SELECT source.* FROM ({refund_payments}) source,scope WHERE source.company_id=scope.company_id AND source.package_id=scope.package_id),\n",
            "  callbacks AS MATERIALIZED (SELECT source.* FROM ({events}) source,scope WHERE source.company_id=scope.company_id AND source.package_id=scope.package_id)\n",
            "  SELECT 'sale' kind,s.id,s.version,s.state FROM sales s WHERE s.state NOT IN ('completed','cancelled','merged')\n",
            "  UNION ALL SELECT 'shift',s.id,s.version,s.state FROM ({shifts}) s,scope WHERE s.company_id=scope.company_id AND s.package_id=scope.package_id AND s.state<>'reviewed'\n",
            "  UNION ALL SELECT 'cash',m.id,0,'unapproved' FROM pos_cash_movements m JOIN ({shifts}) s ON s.id=m.shift_id,scope WHERE s.company_id=scope.company_id AND s.package_id=scope.package_id AND m.approved_by IS NULL\n",
            "  UNION ALL SELECT 'kitchen',f.id,f.version,f.state FROM fulfillments f WHERE f.kind='recipe' AND f.state NOT IN ('delivered','cancelled')\n",
            "  UNION ALL SELECT 'payment',p.id,p.version,p.state FROM payments p WHERE p.state IN ('requested','pending','exception')\n",
            "  UNION ALL SELECT 'refund',r.id,r.version,r.state FROM refunds r WHERE r.state NOT IN ('completed','rejected')\n",
            "  UNION ALL SELECT 'refund-payment',p.id,0,p.state FROM payouts p WHERE p.state<>'succeeded'\n",
            "  UNION ALL SELECT 'room-reversal',r.id,r.version,r.state FROM pos_room_charge_reversal_requests r JOIN sales s ON s.id=r.sale_id AND s.company_id=r.company_id WHERE r.state IN ('requested','pos_approved')\n",
            "  UNION ALL SELECT 'callback',e.id,0,'unapplied' FROM callbacks e WHERE e.applied_at IS NULL\n",
            "  UNION ALL SELECT 'job',j.id,j.attempts,j.state FROM pos_jobs j,scope WHERE j.company_id=scope.company_id AND j.state<>'completed' AND (\n",
            "    j.kind='complete_sale' AND EXISTS(SELECT 1 FROM sales s WHERE s.id=j.record_id)\n",
            "    OR j.kind IN ('checkout','check_payment') AND EXISTS(SELECT 1 FROM payments p WHERE p.id=j.record_id)\n",
            "    OR j.kind='refund' AND EXISTS(SELECT 1 FROM payouts p WHERE p.id=j.record_id)\n",
            "    OR j.kind='event' AND EXISTS(SELECT 1 FROM callbacks e WHERE e.id=j.record_id))\n",
            "  ORDER BY kind,id",
        ),
        sales = f.sales,
        stock_fulfillments = f.stock_fulfillments,
        payments = f.payments,
        refunds = f.refunds,
        refund_payments = f.refund_payments,
        events = f.events,
        shifts = f.shifts,
    ))
}

fn package_facts() -> String {
    concat!(
        "WITH scope AS (SELECT ?::text company_id)\n",
        "    SELECT 'sale' kind,s.id,s.version,s.state FROM pos_sales s,scope WHERE s.company_id=scope.company_id AND s.state NOT IN ('completed','cancelled','merged')\n",
        "    UNION ALL SELECT 'shift',s.id,s.version,s.state FROM pos_shifts s,scope WHERE s.company_id=scope.company_id AND s.state<>'reviewed'\n",
        "    UNION ALL SELECT 'cash',m.id,0,'unapproved' FROM pos_cash_movements m JOIN pos_shifts s ON s.id=m.shift_id,scope WHERE s.company_id=scope.company_id AND m.approved_by IS NULL\n",
        "    UNION ALL SELECT 'kitchen',f.id,f.version,f.state FROM pos_fulfillments f,scope WHERE f.company_id=scope.company_id AND f.kind='recipe' AND f.state NOT IN ('delivered','cancelled')\n",
        "    UNION ALL SELECT 'payment',p.id,p.version,p.state FROM pos_payments p,scope WHERE p.company_id=scope.company_id AND p.state IN ('requested','pending','exception')\n",
        "    UNION ALL SELECT 'refund',r.id,r.version,r.state FROM pos_refunds r,scope WHERE r.company_id=scope.company_id AND r.state NOT IN ('completed','rejected')\n",
        "    UNION ALL SELECT 'refund-payment',rp.id,0,rp.state FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id,scope WHERE r.company_id=scope.company_id AND rp.state<>'succeeded'\n",
        "    UNION ALL SELECT 'room-reversal',r.id,r.version,r.state FROM pos_room_charge_reversal_requests r,scope WHERE r.company_id=scope.company_id AND r.state IN ('requested','pos_approved')\n",
        "    UNION ALL SELECT 'job',j.id,j.attempts,j.state FROM pos_jobs j,scope WHERE j.company_id=scope.company_id AND j.state<>'completed'\n",
        "    UNION ALL SELECT 'callback',e.id,0,'unapplied' FROM pos_stripe_events e,scope WHERE e.company_id=scope.company_id AND e.applied_at IS NULL\n",
        "    UNION ALL SELECT 'provider',c.company_id,c.version,c.state FROM pos_connections c,scope WHERE c.company_id=scope.company_id AND c.account_id IS NOT NULL AND c.state<>'disconnected'\n",
        "    UNION ALL SELECT 'provider-settlement',s.id,0,s.state FROM pos_settlements s,scope WHERE s.company_id=scope.company_id AND (s.state IN ('unmapped','pending','exception','needs_response','under_review','warning_needs_response','warning_under_review') OR s.journal_id IS NULL AND s.amount_minor<>0)\n",
        "    UNION ALL SELECT 'day',concat(d.branch_id,':',d.business_date),0,'unconfirmed' FROM (\n",
        "      SELECT company_id,branch_id,business_date FROM pos_sales UNION SELECT company_id,branch_id,business_date FROM pos_shifts\n",
        "    ) d,scope WHERE d.company_id=scope.company_id AND NOT EXISTS(SELECT 1 FROM pos_day_closes c WHERE c.company_id=d.company_id AND c.branch_id=d.branch_id AND c.business_date=d.business_date AND c.state='confirmed')\n",
        "    UNION ALL SELECT 'day-preview',c.id,c.version,c.state FROM pos_day_closes c,scope WHERE c.company_id=scope.company_id AND c.state<>'confirmed'\n",
        "    ORDER BY kind,id",
    )
    .to_owned()
}

fn pms_package_facts() -> String {
    format!(
        concat!(
            "WITH scope AS (SELECT ?::text company_id),\n",
            "    hotels AS MATERIALIZED (SELECT p.id FROM ({properties}) p,scope WHERE p.company_id=scope.company_id),\n",
            "    linked_sales AS MATERIALIZED (SELECT s.* FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id,scope WHERE s.company_id=scope.company_id\n",
            "      AND (r.property_id IN (SELECT id FROM hotels) OR EXISTS(SELECT 1 FROM ({room_charges}) c WHERE c.company_id=s.company_id AND c.source_key LIKE 'pos:'||s.id||':%')))\n",
            "    SELECT 'sale' kind,s.id,s.version,s.state FROM linked_sales s WHERE s.state NOT IN ('completed','cancelled','merged')\n",
            "    UNION ALL SELECT 'shift',s.id,s.version,s.state FROM pos_shifts s JOIN pos_registers r ON r.id=s.register_id,scope WHERE s.company_id=scope.company_id AND r.property_id IN (SELECT id FROM hotels) AND s.state<>'reviewed'\n",
            "    UNION ALL SELECT 'kitchen',f.id,f.version,f.state FROM pos_fulfillments f JOIN pos_lines l ON l.fulfillment_id=f.id JOIN linked_sales s ON s.id=l.sale_id WHERE f.state NOT IN ('delivered','cancelled') AND f.kind='recipe'\n",
            "    UNION ALL SELECT 'payment',p.id,p.version,p.state FROM pos_payments p JOIN linked_sales s ON s.id=p.sale_id WHERE p.state IN ('requested','pending','exception')\n",
            "    UNION ALL SELECT 'refund',r.id,r.version,r.state FROM pos_refunds r JOIN linked_sales s ON s.id=r.sale_id WHERE r.state NOT IN ('completed','rejected')\n",
            "    UNION ALL SELECT 'room-reversal',r.id,r.version,r.state FROM pos_room_charge_reversal_requests r,scope WHERE r.company_id=scope.company_id AND r.property_id IN (SELECT id FROM hotels) AND r.state IN ('requested','pos_approved')\n",
            "    UNION ALL SELECT 'provider',c.company_id,c.version,c.state FROM pos_connections c,scope WHERE c.company_id=scope.company_id AND c.account_id IS NOT NULL AND c.state<>'disconnected'\n",
            "    UNION ALL SELECT 'job',j.id,j.attempts,j.state FROM pos_jobs j,scope WHERE j.company_id=scope.company_id AND j.state<>'completed'\n",
            "    UNION ALL SELECT 'callback',e.id,0,'unapplied' FROM pos_stripe_events e,scope WHERE e.company_id=scope.company_id AND e.applied_at IS NULL\n",
            "    UNION ALL SELECT 'provider-settlement',s.id,0,s.state FROM pos_settlements s,scope WHERE s.company_id=scope.company_id AND (s.state IN ('unmapped','pending','exception','needs_response','under_review','warning_needs_response','warning_under_review') OR s.journal_id IS NULL AND s.amount_minor<>0)\n",
            "    ORDER BY kind,id",
        ),
        properties = PMS_PROPERTIES,
        room_charges = PMS_ROOM_CHARGES,
    )
}

static ADDON_RESTAURANT_SQL: LazyLock<String> =
    LazyLock::new(|| addon_facts(POS_RESTAURANT).expect("registered addon"));
static ADDON_HOTEL_SQL: LazyLock<String> =
    LazyLock::new(|| addon_facts(POS_HOTEL_CHARGE).expect("registered addon"));
static PACKAGE_SQL: LazyLock<String> = LazyLock::new(package_facts);
static PMS_PACKAGE_SQL: LazyLock<String> = LazyLock::new(pms_package_facts);

/// Authoritative, parameter-free SQL for one mounted obligation read kind.
/// Company/package scope is supplied only as positional values by the bridge.
pub fn sql(kind: &str) -> Option<&'static str> {
    Some(match kind {
        ADDON_RESTAURANT => ADDON_RESTAURANT_SQL.as_str(),
        ADDON_HOTEL => ADDON_HOTEL_SQL.as_str(),
        PACKAGE => PACKAGE_SQL.as_str(),
        PMS_PACKAGE => PMS_PACKAGE_SQL.as_str(),
        _ => return None,
    })
}

/// Owner-owned addon settlement source projections. These are SQL text, not
/// runnable row reads: Finance, Inventory and PMS readers embed them in their
/// own queries, so those cross-owner callers remain in TypeScript. Fail-closed
/// on any package id outside the two registered addons.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementSources {
    pub sales: String,
    pub payments: String,
    pub fulfillments: String,
    pub reservations: String,
    pub stock_moves: String,
    pub required_postings: String,
    pub postings: String,
}

pub fn settlement_sources(package_id: &str) -> Option<SettlementSources> {
    let f = addon_fragments(package_id)?;
    let required_postings = format!(
        concat!(
            "SELECT s.package_id,s.company_id,prefix||s.id source_key FROM ({sales}) s CROSS JOIN (VALUES ('sale:'),('allocate:')) source(prefix) WHERE s.invoice_id IS NOT NULL AND s.total_minor>0\n",
            "    UNION SELECT p.package_id,p.company_id,'receipt:'||p.id FROM ({payments}) p WHERE p.state='succeeded' AND p.amount_minor>0\n",
            "    UNION SELECT f.package_id,f.company_id,'consume:'||f.id FROM ({stock_fulfillments}) f WHERE f.cost_minor>0\n",
            "    UNION SELECT f.package_id,f.company_id,'deliver:'||f.id FROM ({stock_fulfillments}) f WHERE f.kind='recipe' AND f.state='delivered' AND f.cost_minor>0\n",
            "    UNION SELECT f.package_id,f.company_id,'waste:'||f.id FROM ({stock_fulfillments}) f WHERE f.kind='recipe' AND f.state='cancelled' AND f.cost_minor>0\n",
            "    UNION SELECT r.package_id,r.company_id,'refund:'||r.id FROM ({refunds}) r WHERE r.state IN ('processing','completed') AND r.amount_minor>0\n",
            "    UNION SELECT rp.package_id,rp.company_id,'refund-payment:'||rp.id FROM ({refund_payments}) rp WHERE rp.state='succeeded'\n",
            "    UNION SELECT s.package_id,s.company_id,'room-reversal:'||s.id FROM ({sales}) s JOIN pos_room_charge_reversals r ON r.sale_id=s.id AND r.company_id=s.company_id WHERE r.restored_cost_minor+r.wasted_cost_minor>0",
        ),
        sales = f.sales,
        payments = f.payments,
        stock_fulfillments = f.stock_fulfillments,
        refunds = f.refunds,
        refund_payments = f.refund_payments,
    );
    Some(SettlementSources {
        sales: format!(
            "SELECT package_id,company_id,id,version,state,invoice_id,currency,total_minor,accounts_json FROM ({sales}) source",
            sales = f.sales,
        ),
        payments: format!(
            "SELECT package_id,company_id,id,sale_id,state,version,finance_payment_id,amount_minor,method FROM ({payments}) source",
            payments = f.payments,
        ),
        fulfillments: f.stock_fulfillments.clone(),
        reservations: format!(
            "SELECT f.package_id,f.company_id,r.fulfillment_id,r.warehouse_id,r.product_id,r.quantity_milli,f.version FROM pos_reservations r JOIN ({stock_fulfillments}) f ON f.id=r.fulfillment_id",
            stock_fulfillments = f.stock_fulfillments,
        ),
        stock_moves: format!(
            "SELECT f.package_id,m.* FROM pos_stock_moves m JOIN ({stock_fulfillments}) f ON f.id=m.fulfillment_id AND f.company_id=m.company_id",
            stock_fulfillments = f.stock_fulfillments,
        ),
        required_postings,
        postings: format!(
            "SELECT source.package_id,source.source_id expected_source_id,source.currency expected_currency,source.accounts_json expected_accounts_json,p.* FROM pos_postings p JOIN ({posting_keys}) source ON source.company_id=p.company_id AND source.source_key=p.source_key",
            posting_keys = f.posting_keys,
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obligation_kinds_are_allow_listed_select_only_and_scoped() {
        assert_eq!(KINDS.len(), 4);
        for kind in KINDS {
            assert!(supported(kind), "kind must be allow listed: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("WITH scope AS (SELECT "), "kind must be a scoped read: {kind}");
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
                assert!(!stmt.contains(forbidden), "kind must not mutate: {kind} contains {forbidden}");
            }
        }
        assert!(sql("pos_obligation_unlisted").is_none());
        assert!(!supported("pos_obligation_unlisted"));
    }

    #[test]
    fn addon_scope_requires_a_registered_package() {
        for bad in ["", "pos", "pms", "pos-restaurant'; DROP TABLE pos_sales; --"] {
            assert!(addon_facts(bad).is_none(), "package {bad:?} must be rejected");
            assert!(settlement_sources(bad).is_none(), "package {bad:?} must be rejected");
        }
        for ok in [POS_RESTAURANT, POS_HOTEL_CHARGE] {
            assert!(addon_facts(ok).is_some());
            assert!(settlement_sources(ok).is_some());
        }
    }
}
