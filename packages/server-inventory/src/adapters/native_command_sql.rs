//! Native write statements owned by this crate. Generated from the retired
//! TypeScript write literals with full-parity positional payloads. The axum
//! executor runs these in the caller-owned serializable unit.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeStatement {
    pub kind: &'static str,
    pub sql: &'static str,
    pub bind_count: usize,
    pub scope: &'static str,
}

const STATEMENTS: &[NativeStatement] = &[
    NativeStatement {
        kind: "inventory_stmt_1",
        sql: r#"INSERT INTO inventory_order_reservations(id,order_line_id,warehouse_id,product_id,quantity_milli,expires_at,created_by,reason,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)"#,
        bind_count: 9,
        scope: "holding",
    }, // commerce-facts.ts
    NativeStatement {
        kind: "inventory_stmt_2",
        sql: r#"UPDATE inventory_order_reservations SET released_milli=released_milli+$1,release_reason=$2 WHERE id=$3"#,
        bind_count: 3,
        scope: "holding",
    }, // commerce-facts.ts
    NativeStatement {
        kind: "inventory_stmt_3",
        sql: r#"INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor) VALUES($1,$2,$3,$4) ON CONFLICT(lot_id,warehouse_id) DO UPDATE SET quantity_milli=inventory_lot_balances.quantity_milli+EXCLUDED.quantity_milli,value_minor=inventory_lot_balances.value_minor+EXCLUDED.value_minor,version=inventory_lot_balances.version+1"#,
        bind_count: 4,
        scope: "holding",
    }, // commerce-facts.ts
    NativeStatement {
        kind: "inventory_stmt_4",
        sql: r#"UPDATE inventory_lots SET status='active' WHERE id=$1"#,
        bind_count: 1,
        scope: "holding",
    }, // commerce-facts.ts
    NativeStatement {
        kind: "inventory_stmt_5",
        sql: r#"UPDATE inventory_lot_balances SET quantity_milli=quantity_milli-$1,value_minor=value_minor-$2,version=version+1 WHERE lot_id=$3 AND warehouse_id=$4"#,
        bind_count: 4,
        scope: "holding",
    }, // commerce-facts.ts
    NativeStatement {
        kind: "inventory_stmt_6",
        sql: r#"UPDATE inventory_lots SET status=CASE WHEN EXISTS(SELECT 1 FROM inventory_lot_balances WHERE lot_id=$1 AND quantity_milli>0) THEN 'active' ELSE 'returned' END WHERE id=$2"#,
        bind_count: 2,
        scope: "holding",
    }, // commerce-facts.ts
    NativeStatement {
        kind: "inventory_stmt_7",
        sql: r#"INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,source_id,quantity_milli,value_minor,movement_date,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"#,
        bind_count: 10,
        scope: "holding",
    }, // commerce-facts.ts
    NativeStatement {
        kind: "inventory_stmt_8",
        sql: r#"UPDATE inventory_order_reservations SET released_milli=quantity_milli-consumed_milli,release_reason='Source order cancelled' WHERE order_line_id IN (SELECT id FROM (SELECT "id","order_id","product_id","description","quantity_milli","fulfilled_milli","price_minor","tax_bps","subtotal_minor","tax_minor","ordinal","list_price_minor","discount_bps","item_type" FROM commerce_order_lines) commerce_order_lines WHERE order_id=$1) AND quantity_milli>consumed_milli+released_milli"#,
        bind_count: 1,
        scope: "holding",
    }, // commerce-facts.ts (resolved commerce order-lines projection)
    NativeStatement {
        kind: "inventory_stmt_9",
        sql: r#"UPDATE commerce_stock SET quantity_milli=quantity_milli+$1,value_minor=value_minor+$2,version=version+1 WHERE warehouse_id=$3 AND product_id=$4"#,
        bind_count: 4,
        scope: "holding",
    }, // commerce-stock-owner-effects.ts
    NativeStatement {
        kind: "inventory_stmt_10",
        sql: r#"INSERT INTO commerce_stock(warehouse_id,product_id) VALUES($1,$2)"#,
        bind_count: 2,
        scope: "holding",
    }, // commerce-stock-owner-effects.ts
    NativeStatement {
        kind: "inventory_stmt_11",
        sql: r#"INSERT INTO commerce_stock_moves VALUES($1,$2,$3,$4,$5,$6,$7)"#,
        bind_count: 7,
        scope: "holding",
    }, // commerce-stock-owner-effects.ts
    NativeStatement {
        kind: "inventory_stmt_12",
        sql: r#"UPDATE commerce_stock SET quantity_milli=quantity_milli-$1,value_minor=value_minor-$2,version=version+1 WHERE warehouse_id=$3 AND product_id=$4"#,
        bind_count: 4,
        scope: "holding",
    }, // commerce-stock-owner-effects.ts
    NativeStatement {
        kind: "inventory_stmt_13",
        sql: r#"UPDATE commerce_stock SET quantity_milli=$1,value_minor=$2,version=version+1 WHERE warehouse_id=$3 AND product_id=$4"#,
        bind_count: 4,
        scope: "holding",
    }, // commerce-stock-owner-effects.ts
    NativeStatement {
        kind: "inventory_stmt_14",
        sql: r#"UPDATE commerce_stock SET value_minor=value_minor+$1,version=version+1 WHERE warehouse_id=$2 AND product_id=$3"#,
        bind_count: 3,
        scope: "holding",
    }, // commerce-stock-owner-effects.ts
    NativeStatement {
        kind: "inventory_stmt_15",
        sql: r#"INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES($1,$2,$3,$4,1) ON CONFLICT(warehouse_id,product_id) DO UPDATE SET quantity_milli=commerce_stock.quantity_milli+excluded.quantity_milli,value_minor=commerce_stock.value_minor+excluded.value_minor,version=commerce_stock.version+1"#,
        bind_count: 4,
        scope: "holding",
    }, // commerce-stock-owner-effects.ts
];

pub fn statement(kind: &str) -> Option<&'static NativeStatement> {
    STATEMENTS.iter().find(|s| s.kind == kind)
}

pub fn supported(kind: &str) -> bool {
    statement(kind).is_some()
}

/// Owner-owned read plans. A read statement carries no receipt/audit/outbox
/// ledger: the native route plans the exact SQL and the caller executes it on
/// its own already-open unit, so it observes uncommitted effects inside the
/// caller's serializable transaction. This is the native successor of the
/// retired TypeScript `readRecordSupplementDataRows2` SQL literal and mirrors
/// the Files/Commerce caller-unit read port.
pub const RD_LEDGER_SUPPLEMENT_ROWS: &str = "inventory_rd_ledger_supplement_rows";
const SQL_RD_LEDGER_SUPPLEMENT_ROWS: &str =
    "SELECT * FROM inventory_ledger WHERE warehouse_id=? AND product_id=? ORDER BY movement_date DESC,id DESC LIMIT 31 OFFSET ?";
const READ_LEDGER_SUPPLEMENT: NativeStatement = NativeStatement {
    kind: RD_LEDGER_SUPPLEMENT_ROWS,
    sql: SQL_RD_LEDGER_SUPPLEMENT_ROWS,
    bind_count: 3,
    scope: "holding",
};

/// Native successor of the TypeScript `readRecordSupplementDataRows` read.
/// Same owner statement, planned here and executed on the caller's unit.
pub const RD_LOT_BALANCE_SUPPLEMENT_ROWS: &str = "inventory_rd_lot_balance_supplement_rows";
const SQL_RD_LOT_BALANCE_SUPPLEMENT_ROWS: &str = r#"SELECT b.*,w.name warehouse_name FROM inventory_lot_balances b JOIN (SELECT "id","company_id","branch_id","code","name","status","version" FROM commerce_warehouses) w ON w.id=b.warehouse_id WHERE b.lot_id=? AND w.company_id=? AND (?::text IS NULL OR w.branch_id=?) ORDER BY w.name LIMIT 31 OFFSET ?"#;
const READ_LOT_BALANCE_SUPPLEMENT: NativeStatement = NativeStatement {
    kind: RD_LOT_BALANCE_SUPPLEMENT_ROWS,
    sql: SQL_RD_LOT_BALANCE_SUPPLEMENT_ROWS,
    bind_count: 5,
    scope: "holding",
};

/// Native successor of the TypeScript `readRecordSupplementDataRow` ledger
/// snapshot read. Projections are inlined exactly as the retired owner literal
/// resolved them; the bridge executes this on the caller's unit.
pub const RD_LEDGER_SUPPLEMENT_ROW: &str = "inventory_rd_ledger_supplement_row";
const SQL_RD_LEDGER_SUPPLEMENT_ROW: &str = r#"SELECT m.*,p.sku,p.name,p.unit,w.name warehouse_name,c.base_currency currency FROM inventory_ledger m JOIN (SELECT "id","company_id","sku","name","unit","price_minor","tax_bps","status","version","tracking_mode","item_type" FROM commerce_products) p ON p.id=m.product_id JOIN (SELECT "id","company_id","branch_id","code","name","status","version" FROM commerce_warehouses) w ON w.id=m.warehouse_id LEFT JOIN (SELECT "company_id","chart_id","currency","receivable_account_id","payable_account_id","revenue_account_id","inventory_account_id","clearing_account_id","cogs_account_id","input_tax_account_id","output_tax_account_id","cash_account_id" FROM commerce_settings) s ON s.company_id=w.company_id LEFT JOIN (SELECT "id","company_id","name","base_currency","accounting_basis","status","version","created_by","created_at","updated_at" FROM finance_charts) c ON c.id=s.chart_id WHERE m.id=? AND w.company_id=? AND (?::text IS NULL OR w.branch_id=?)"#;
const READ_LEDGER_SUPPLEMENT_ROW: NativeStatement = NativeStatement {
    kind: RD_LEDGER_SUPPLEMENT_ROW,
    sql: SQL_RD_LEDGER_SUPPLEMENT_ROW,
    bind_count: 4,
    scope: "holding",
};

/// Native successor of the TypeScript `readRecordSupplementDataRow2` lot
/// snapshot read (EXISTS-scoped to an authorized lot balance).
pub const RD_LOT_SUPPLEMENT_ROW: &str = "inventory_rd_lot_supplement_row";
const SQL_RD_LOT_SUPPLEMENT_ROW: &str = r#"SELECT l.*,p.sku,p.name,p.unit FROM inventory_lots l JOIN (SELECT "id","company_id","sku","name","unit","price_minor","tax_bps","status","version","tracking_mode","item_type" FROM commerce_products) p ON p.id=l.product_id WHERE l.id=? AND l.company_id=? AND EXISTS(SELECT 1 FROM inventory_lot_balances b JOIN (SELECT "id","company_id","branch_id","code","name","status","version" FROM commerce_warehouses) w ON w.id=b.warehouse_id WHERE b.lot_id=l.id AND w.company_id=? AND (?::text IS NULL OR w.branch_id=?))"#;
const READ_LOT_SUPPLEMENT_ROW: NativeStatement = NativeStatement {
    kind: RD_LOT_SUPPLEMENT_ROW,
    sql: SQL_RD_LOT_SUPPLEMENT_ROW,
    bind_count: 5,
    scope: "holding",
};

/// Native successor of the TypeScript `readRecordSupplementDataRow3` stock
/// snapshot read (reserved/quarantined facts computed on the caller's unit).
pub const RD_STOCK_SUPPLEMENT_ROW: &str = "inventory_rd_stock_supplement_row";
const SQL_RD_STOCK_SUPPLEMENT_ROW: &str = r#"SELECT s.*,p.sku,p.name,p.unit,w.name warehouse_name, COALESCE((SELECT SUM(r.quantity_milli) FROM inventory_active_reservations r WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id),0) reserved_milli, inventory_quarantined_quantity(s.warehouse_id,s.product_id) quarantined_milli FROM commerce_stock s JOIN (SELECT "id","company_id","sku","name","unit","price_minor","tax_bps","status","version","tracking_mode","item_type" FROM commerce_products) p ON p.id=s.product_id JOIN (SELECT "id","company_id","branch_id","code","name","status","version" FROM commerce_warehouses) w ON w.id=s.warehouse_id WHERE s.product_id=? AND s.warehouse_id=? AND w.company_id=? AND (?::text IS NULL OR w.branch_id=?)"#;
const READ_STOCK_SUPPLEMENT_ROW: NativeStatement = NativeStatement {
    kind: RD_STOCK_SUPPLEMENT_ROW,
    sql: SQL_RD_STOCK_SUPPLEMENT_ROW,
    bind_count: 5,
    scope: "holding",
};

/// Native successor of the TypeScript document-posting preview `stockCount`
/// single-row read. The owner plans this literal and the caller executes it on
/// its own already-open unit; this crate carries no TypeScript SQL for it.
pub const RD_STOCK_COUNT_ROW: &str = "inventory_rd_stock_count_row";
const SQL_RD_STOCK_COUNT_ROW: &str = "SELECT * FROM inventory_counts WHERE company_id=? AND id=?";
const READ_STOCK_COUNT_ROW: NativeStatement = NativeStatement {
    kind: RD_STOCK_COUNT_ROW,
    sql: SQL_RD_STOCK_COUNT_ROW,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript document-posting preview `stockDeltas`
/// array read. The owner plans this literal and the caller executes it on its
/// own already-open unit; this crate carries no TypeScript SQL for it.
pub const RD_STOCK_COUNT_DELTAS: &str = "inventory_rd_stock_count_deltas";
const SQL_RD_STOCK_COUNT_DELTAS: &str =
    "SELECT counted_value_minor-snapshot_value_minor delta FROM inventory_count_lines WHERE count_id=?";
const READ_STOCK_COUNT_DELTAS: NativeStatement = NativeStatement {
    kind: RD_STOCK_COUNT_DELTAS,
    sql: SQL_RD_STOCK_COUNT_DELTAS,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the TypeScript `pos-core-facts` active-reservation
/// total read. Owner-local: the statement touches only Inventory's own table
/// and the caller executes it on its already-open unit with the same binds.
pub const RD_ACTIVE_RESERVATIONS_TOTAL: &str = "inventory_rd_active_reservations_total";
const SQL_RD_ACTIVE_RESERVATIONS_TOTAL: &str =
    "SELECT COALESCE(SUM(quantity_milli),0) n FROM inventory_active_reservations WHERE warehouse_id=? AND product_id=?";
const READ_ACTIVE_RESERVATIONS_TOTAL: NativeStatement = NativeStatement {
    kind: RD_ACTIVE_RESERVATIONS_TOTAL,
    sql: SQL_RD_ACTIVE_RESERVATIONS_TOTAL,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript `pos-core-facts` ledger movement read.
/// Owner-local and planned here; the caller runs it on its own unit and takes
/// the first row, exactly as the retired `.first()` literal did.
pub const RD_LEDGER_MOVEMENT_AFTER: &str = "inventory_rd_ledger_movement_after";
const SQL_RD_LEDGER_MOVEMENT_AFTER: &str =
    "SELECT id FROM inventory_ledger WHERE warehouse_id=? AND product_id=? AND movement_date>? LIMIT 1";
const READ_LEDGER_MOVEMENT_AFTER: NativeStatement = NativeStatement {
    kind: RD_LEDGER_MOVEMENT_AFTER,
    sql: SQL_RD_LEDGER_MOVEMENT_AFTER,
    bind_count: 3,
    scope: "holding",
};

/// Native successor of the TypeScript `attachments-operational-facts`
/// `readInventoryTransfers1` read. Owner-local: the statement touches only
/// Inventory's own `inventory_transfers` table and the caller executes it on
/// its already-open unit with the same binds, taking the first row.
pub const RD_ATTACHMENT_OPERATIONAL_TRANSFER: &str = "inventory_rd_attachment_operational_transfer";
const SQL_RD_ATTACHMENT_OPERATIONAL_TRANSFER: &str =
    "SELECT company_id,source_branch_id,target_branch_id,NULL::text department_id,version,status,created_by,prepared_by,delivery_mode FROM inventory_transfers WHERE id=? AND company_id=?";
const READ_ATTACHMENT_OPERATIONAL_TRANSFER: NativeStatement = NativeStatement {
    kind: RD_ATTACHMENT_OPERATIONAL_TRANSFER,
    sql: SQL_RD_ATTACHMENT_OPERATIONAL_TRANSFER,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript `attachments-policy-facts` `readTransit`
/// read. Owner-local: the statement touches only Inventory's own
/// `inventory_transit` table and the caller executes it on its already-open
/// unit with the same binds, taking the first row.
pub const RD_TRANSIT_ATTACHMENT_DESCRIPTOR: &str = "inventory_rd_transit_attachment_descriptor";
const SQL_RD_TRANSIT_ATTACHMENT_DESCRIPTOR: &str =
    "SELECT company_id,source_branch_id,target_branch_id,version FROM inventory_transit WHERE id=? AND company_id=?";
const READ_TRANSIT_ATTACHMENT_DESCRIPTOR: NativeStatement = NativeStatement {
    kind: RD_TRANSIT_ATTACHMENT_DESCRIPTOR,
    sql: SQL_RD_TRANSIT_ATTACHMENT_DESCRIPTOR,
    bind_count: 2,
    scope: "holding",
};

/// Native successors of the TypeScript `attachments-policy-facts`
/// `readDescriptor` read. The retired adapter interpolated a per-target-type
/// descriptor (table, branch column, version column, NULL department, empty
/// extra); every descriptor in use resolves to one of these five owner-local
/// fixed statements. The caller executes on its own unit and takes the first
/// row, exactly as the retired `.first()` literal did.
pub const RD_ATTACHMENT_DESCRIPTOR_LANDED_COST: &str = "inventory_rd_attachment_descriptor_landed_cost";
const SQL_RD_ATTACHMENT_DESCRIPTOR_LANDED_COST: &str =
    "SELECT company_id,branch_id AS branch_id,NULL AS department_id,version AS version FROM inventory_landed_costs WHERE id=? AND company_id=?";
const READ_ATTACHMENT_DESCRIPTOR_LANDED_COST: NativeStatement = NativeStatement {
    kind: RD_ATTACHMENT_DESCRIPTOR_LANDED_COST,
    sql: SQL_RD_ATTACHMENT_DESCRIPTOR_LANDED_COST,
    bind_count: 2,
    scope: "holding",
};

pub const RD_ATTACHMENT_DESCRIPTOR_STOCK_COUNT: &str = "inventory_rd_attachment_descriptor_stock_count";
const SQL_RD_ATTACHMENT_DESCRIPTOR_STOCK_COUNT: &str =
    "SELECT company_id,branch_id AS branch_id,NULL AS department_id,version AS version FROM inventory_counts WHERE id=? AND company_id=?";
const READ_ATTACHMENT_DESCRIPTOR_STOCK_COUNT: NativeStatement = NativeStatement {
    kind: RD_ATTACHMENT_DESCRIPTOR_STOCK_COUNT,
    sql: SQL_RD_ATTACHMENT_DESCRIPTOR_STOCK_COUNT,
    bind_count: 2,
    scope: "holding",
};

pub const RD_ATTACHMENT_DESCRIPTOR_TRANSFER: &str = "inventory_rd_attachment_descriptor_transfer";
const SQL_RD_ATTACHMENT_DESCRIPTOR_TRANSFER: &str =
    "SELECT company_id,source_branch_id AS branch_id,NULL AS department_id,version AS version FROM inventory_transfers WHERE id=? AND company_id=?";
const READ_ATTACHMENT_DESCRIPTOR_TRANSFER: NativeStatement = NativeStatement {
    kind: RD_ATTACHMENT_DESCRIPTOR_TRANSFER,
    sql: SQL_RD_ATTACHMENT_DESCRIPTOR_TRANSFER,
    bind_count: 2,
    scope: "holding",
};

pub const RD_ATTACHMENT_DESCRIPTOR_TRANSIT: &str = "inventory_rd_attachment_descriptor_transit";
const SQL_RD_ATTACHMENT_DESCRIPTOR_TRANSIT: &str =
    "SELECT company_id,source_branch_id AS branch_id,NULL AS department_id,version AS version FROM inventory_transit WHERE id=? AND company_id=?";
const READ_ATTACHMENT_DESCRIPTOR_TRANSIT: NativeStatement = NativeStatement {
    kind: RD_ATTACHMENT_DESCRIPTOR_TRANSIT,
    sql: SQL_RD_ATTACHMENT_DESCRIPTOR_TRANSIT,
    bind_count: 2,
    scope: "holding",
};

pub const RD_ATTACHMENT_DESCRIPTOR_LOT: &str = "inventory_rd_attachment_descriptor_lot";
const SQL_RD_ATTACHMENT_DESCRIPTOR_LOT: &str =
    "SELECT company_id,NULL AS branch_id,NULL AS department_id,version AS version FROM inventory_lots WHERE id=? AND company_id=?";
const READ_ATTACHMENT_DESCRIPTOR_LOT: NativeStatement = NativeStatement {
    kind: RD_ATTACHMENT_DESCRIPTOR_LOT,
    sql: SQL_RD_ATTACHMENT_DESCRIPTOR_LOT,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript `commerce-attachment-sources`
/// `commerceAttachmentStockSourceImage` read. Owner-local: it aggregates only
/// Inventory's own reservation, allocation, lot and lot-balance tables, and the
/// caller executes it on its already-open unit with the same four binds.
pub const RD_COMMERCE_ATTACHMENT_STOCK_IMAGE: &str = "inventory_rd_commerce_attachment_stock_image";
const SQL_RD_COMMERCE_ATTACHMENT_STOCK_IMAGE: &str = r#"WITH allocations AS MATERIALIZED(SELECT a.* FROM inventory_fulfillment_lot_allocations a WHERE a.fulfillment_line_id IN(SELECT jsonb_array_elements_text(?::jsonb))),lots AS MATERIALIZED(SELECT l.* FROM inventory_lots l WHERE l.company_id=? AND(l.id IN(SELECT lot_id FROM allocations) OR l.id IN(SELECT jsonb_array_elements_text(?::jsonb))))
 SELECT jsonb_build_object('reservations',(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM inventory_order_reservations r WHERE r.order_line_id IN(SELECT jsonb_array_elements_text(?::jsonb))),'allocations',(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM allocations a),'lots',(SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM lots l),'balances',(SELECT jsonb_agg(to_jsonb(b) ORDER BY b.lot_id,b.warehouse_id) FROM inventory_lot_balances b JOIN lots l ON l.id=b.lot_id))::text image"#;
const READ_COMMERCE_ATTACHMENT_STOCK_IMAGE: NativeStatement = NativeStatement {
    kind: RD_COMMERCE_ATTACHMENT_STOCK_IMAGE,
    sql: SQL_RD_COMMERCE_ATTACHMENT_STOCK_IMAGE,
    bind_count: 4,
    scope: "holding",
};

/// Native successor of the TypeScript `commerce-facts` `readInventoryInTransit1`
/// read. Owner-local: it aggregates only Inventory's own `inventory_in_transit`
/// table and the caller executes it on its already-open unit with the same four
/// binds, returning the full `SqlResult` array like the retired `.all()`.
pub const RD_IN_TRANSIT_CURRENCY_TOTALS: &str = "inventory_rd_in_transit_currency_totals";
const SQL_RD_IN_TRANSIT_CURRENCY_TOTALS: &str =
    "SELECT currency,count(*) line_count,SUM(value_minor) value_minor FROM inventory_in_transit WHERE company_id=? AND quantity_milli>0 AND (?::text IS NULL OR source_branch_id=? OR target_branch_id=?) GROUP BY currency ORDER BY currency";
const READ_IN_TRANSIT_CURRENCY_TOTALS: NativeStatement = NativeStatement {
    kind: RD_IN_TRANSIT_CURRENCY_TOTALS,
    sql: SQL_RD_IN_TRANSIT_CURRENCY_TOTALS,
    bind_count: 4,
    scope: "holding",
};

/// Native successor of the TypeScript `commerce-facts`
/// `readInventoryFulfillmentLotAllocations1` read. Owner-local: it touches only
/// Inventory's own allocation and lot tables, and the caller executes it on its
/// already-open unit with the same two binds (so it observes uncommitted
/// effects in a fulfillment write flow).
pub const RD_FULFILLMENT_LOT_ALLOCATION: &str = "inventory_rd_fulfillment_lot_allocation";
const SQL_RD_FULFILLMENT_LOT_ALLOCATION: &str =
    "SELECT a.*,l.tracking_mode FROM inventory_fulfillment_lot_allocations a JOIN inventory_lots l ON l.id=a.lot_id WHERE a.fulfillment_line_id=? AND a.lot_id=?";
const READ_FULFILLMENT_LOT_ALLOCATION: NativeStatement = NativeStatement {
    kind: RD_FULFILLMENT_LOT_ALLOCATION,
    sql: SQL_RD_FULFILLMENT_LOT_ALLOCATION,
    bind_count: 2,
    scope: "holding",
};

pub fn is_read(kind: &str) -> bool {
    matches!(
        kind,
        RD_LEDGER_SUPPLEMENT_ROWS
            | RD_LOT_BALANCE_SUPPLEMENT_ROWS
            | RD_LEDGER_SUPPLEMENT_ROW
            | RD_LOT_SUPPLEMENT_ROW
            | RD_STOCK_SUPPLEMENT_ROW
            | RD_STOCK_COUNT_ROW
            | RD_STOCK_COUNT_DELTAS
            | RD_ACTIVE_RESERVATIONS_TOTAL
            | RD_LEDGER_MOVEMENT_AFTER
            | RD_ATTACHMENT_OPERATIONAL_TRANSFER
            | RD_TRANSIT_ATTACHMENT_DESCRIPTOR
            | RD_ATTACHMENT_DESCRIPTOR_LANDED_COST
            | RD_ATTACHMENT_DESCRIPTOR_STOCK_COUNT
            | RD_ATTACHMENT_DESCRIPTOR_TRANSFER
            | RD_ATTACHMENT_DESCRIPTOR_TRANSIT
            | RD_ATTACHMENT_DESCRIPTOR_LOT
            | RD_COMMERCE_ATTACHMENT_STOCK_IMAGE
            | RD_IN_TRANSIT_CURRENCY_TOTALS
            | RD_FULFILLMENT_LOT_ALLOCATION
    )
}

pub fn read_statement(kind: &str) -> Option<&'static NativeStatement> {
    match kind {
        RD_LEDGER_SUPPLEMENT_ROWS => Some(&READ_LEDGER_SUPPLEMENT),
        RD_LOT_BALANCE_SUPPLEMENT_ROWS => Some(&READ_LOT_BALANCE_SUPPLEMENT),
        RD_LEDGER_SUPPLEMENT_ROW => Some(&READ_LEDGER_SUPPLEMENT_ROW),
        RD_LOT_SUPPLEMENT_ROW => Some(&READ_LOT_SUPPLEMENT_ROW),
        RD_STOCK_SUPPLEMENT_ROW => Some(&READ_STOCK_SUPPLEMENT_ROW),
        RD_STOCK_COUNT_ROW => Some(&READ_STOCK_COUNT_ROW),
        RD_STOCK_COUNT_DELTAS => Some(&READ_STOCK_COUNT_DELTAS),
        RD_ACTIVE_RESERVATIONS_TOTAL => Some(&READ_ACTIVE_RESERVATIONS_TOTAL),
        RD_LEDGER_MOVEMENT_AFTER => Some(&READ_LEDGER_MOVEMENT_AFTER),
        RD_ATTACHMENT_OPERATIONAL_TRANSFER => Some(&READ_ATTACHMENT_OPERATIONAL_TRANSFER),
        RD_TRANSIT_ATTACHMENT_DESCRIPTOR => Some(&READ_TRANSIT_ATTACHMENT_DESCRIPTOR),
        RD_ATTACHMENT_DESCRIPTOR_LANDED_COST => Some(&READ_ATTACHMENT_DESCRIPTOR_LANDED_COST),
        RD_ATTACHMENT_DESCRIPTOR_STOCK_COUNT => Some(&READ_ATTACHMENT_DESCRIPTOR_STOCK_COUNT),
        RD_ATTACHMENT_DESCRIPTOR_TRANSFER => Some(&READ_ATTACHMENT_DESCRIPTOR_TRANSFER),
        RD_ATTACHMENT_DESCRIPTOR_TRANSIT => Some(&READ_ATTACHMENT_DESCRIPTOR_TRANSIT),
        RD_ATTACHMENT_DESCRIPTOR_LOT => Some(&READ_ATTACHMENT_DESCRIPTOR_LOT),
        RD_COMMERCE_ATTACHMENT_STOCK_IMAGE => Some(&READ_COMMERCE_ATTACHMENT_STOCK_IMAGE),
        RD_IN_TRANSIT_CURRENCY_TOTALS => Some(&READ_IN_TRANSIT_CURRENCY_TOTALS),
        RD_FULFILLMENT_LOT_ALLOCATION => Some(&READ_FULFILLMENT_LOT_ALLOCATION),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_statement_is_well_formed() {
        for s in STATEMENTS {
            assert!(
                s.sql.trim_start().starts_with("INSERT")
                    || s.sql.trim_start().starts_with("UPDATE")
                    || s.sql.trim_start().starts_with("DELETE")
            );
            let max = s
                .sql
                .split('$')
                .skip(1)
                .filter_map(|x| {
                    x.chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect::<String>()
                        .parse::<usize>()
                        .ok()
                })
                .max()
                .unwrap_or(0);
            assert_eq!(max, s.bind_count, "{}", s.kind);
        }
    }

    #[test]
    fn covers_every_distinct_retired_statement() {
        assert_eq!(STATEMENTS.len(), 15);
    }

    #[test]
    fn read_plan_is_call_unit_and_ledger_free() {
        for kind in [
            RD_LEDGER_SUPPLEMENT_ROWS,
            RD_LOT_BALANCE_SUPPLEMENT_ROWS,
            RD_LEDGER_SUPPLEMENT_ROW,
            RD_LOT_SUPPLEMENT_ROW,
            RD_STOCK_SUPPLEMENT_ROW,
            RD_STOCK_COUNT_ROW,
            RD_STOCK_COUNT_DELTAS,
            RD_ACTIVE_RESERVATIONS_TOTAL,
            RD_LEDGER_MOVEMENT_AFTER,
            RD_ATTACHMENT_OPERATIONAL_TRANSFER,
            RD_TRANSIT_ATTACHMENT_DESCRIPTOR,
            RD_ATTACHMENT_DESCRIPTOR_LANDED_COST,
            RD_ATTACHMENT_DESCRIPTOR_STOCK_COUNT,
            RD_ATTACHMENT_DESCRIPTOR_TRANSFER,
            RD_ATTACHMENT_DESCRIPTOR_TRANSIT,
            RD_ATTACHMENT_DESCRIPTOR_LOT,
            RD_COMMERCE_ATTACHMENT_STOCK_IMAGE,
            RD_IN_TRANSIT_CURRENCY_TOTALS,
            RD_FULFILLMENT_LOT_ALLOCATION,
        ] {
            assert!(is_read(kind));
            let read = read_statement(kind).expect("read kind");
            let trimmed = read.sql.trim_start();
            assert!(trimmed.starts_with("SELECT") || trimmed.starts_with("WITH"), "{kind}");
            assert_eq!(read.sql.matches('?').count(), read.bind_count, "{kind}");
            assert_eq!(read.scope, "holding", "{kind}");
        }
        assert!(!is_read("inventory_stmt_1"));
        let read = read_statement(RD_LEDGER_SUPPLEMENT_ROWS).expect("read kind");
        assert_eq!(read.bind_count, 3);
        assert!(read_statement("inventory_stmt_1").is_none());
    }

    /// Parity witness for the single-row `records-record-details-owned-facts`
    /// reads: each native plan is the resolved retired owner literal with the
    /// commerce/finance projections inlined and the caller's unit binds intact.
    #[test]
    fn single_row_read_plans_match_retired_owner_statements() {
        let ledger = read_statement(RD_LEDGER_SUPPLEMENT_ROW).expect("read kind");
        assert_eq!(ledger.bind_count, 4);
        assert!(ledger.sql.contains("FROM inventory_ledger m"));
        assert!(ledger.sql.contains("JOIN (SELECT \"id\",\"company_id\",\"sku\",\"name\",\"unit\",\"price_minor\",\"tax_bps\",\"status\",\"version\",\"tracking_mode\",\"item_type\" FROM commerce_products) p"));
        assert!(ledger.sql.contains("LEFT JOIN (SELECT \"id\",\"company_id\",\"name\",\"base_currency\",\"accounting_basis\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\" FROM finance_charts) c"));
        assert!(ledger.sql.ends_with("WHERE m.id=? AND w.company_id=? AND (?::text IS NULL OR w.branch_id=?)"));

        let lot = read_statement(RD_LOT_SUPPLEMENT_ROW).expect("read kind");
        assert_eq!(lot.bind_count, 5);
        assert!(lot.sql.starts_with("SELECT l.*,p.sku,p.name,p.unit FROM inventory_lots l"));
        assert!(lot.sql.contains("EXISTS(SELECT 1 FROM inventory_lot_balances b JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"code\",\"name\",\"status\",\"version\" FROM commerce_warehouses) w ON w.id=b.warehouse_id WHERE b.lot_id=l.id AND w.company_id=? AND (?::text IS NULL OR w.branch_id=?))"));

        let stock = read_statement(RD_STOCK_SUPPLEMENT_ROW).expect("read kind");
        assert_eq!(stock.bind_count, 5);
        assert!(stock.sql.contains("COALESCE((SELECT SUM(r.quantity_milli) FROM inventory_active_reservations r WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id),0) reserved_milli"));
        assert!(stock.sql.contains("inventory_quarantined_quantity(s.warehouse_id,s.product_id) quarantined_milli"));
        assert!(stock.sql.ends_with("WHERE s.product_id=? AND s.warehouse_id=? AND w.company_id=? AND (?::text IS NULL OR w.branch_id=?)"));
    }

    /// Parity witness for the `document-posting-preview-facts` read family: the
    /// single-row stock-count plan and the delta array plan are the exact
    /// statements the retired TypeScript literals bound, with caller-unit binds.
    #[test]
    fn document_posting_preview_read_plans_match_retired_owner_statements() {
        let count = read_statement(RD_STOCK_COUNT_ROW).expect("read kind");
        assert_eq!(count.bind_count, 2);
        assert_eq!(count.sql, "SELECT * FROM inventory_counts WHERE company_id=? AND id=?");

        let deltas = read_statement(RD_STOCK_COUNT_DELTAS).expect("read kind");
        assert_eq!(deltas.bind_count, 1);
        assert_eq!(
            deltas.sql,
            "SELECT counted_value_minor-snapshot_value_minor delta FROM inventory_count_lines WHERE count_id=?"
        );
    }

    /// Parity witness for the owner-local `pos-core-facts` read family: each
    /// native plan is the exact statement the retired TypeScript literals bound,
    /// with caller-unit binds and no cross-owner projection.
    #[test]
    fn pos_core_facts_read_plans_match_retired_owner_statements() {
        let total = read_statement(RD_ACTIVE_RESERVATIONS_TOTAL).expect("read kind");
        assert_eq!(
            total.sql,
            "SELECT COALESCE(SUM(quantity_milli),0) n FROM inventory_active_reservations WHERE warehouse_id=? AND product_id=?"
        );
        assert_eq!(total.bind_count, 2);

        let movement = read_statement(RD_LEDGER_MOVEMENT_AFTER).expect("read kind");
        assert_eq!(
            movement.sql,
            "SELECT id FROM inventory_ledger WHERE warehouse_id=? AND product_id=? AND movement_date>? LIMIT 1"
        );
        assert_eq!(movement.bind_count, 3);
    }

    /// Parity witness for the owner-local `attachments-operational-facts` read:
    /// the native plan is the exact statement the retired TypeScript literal
    /// bound on the inventory owner's `inventory_transfers` table, with the
    /// caller-unit binds intact and no cross-owner projection.
    #[test]
    fn attachment_operational_transfer_read_plan_matches_retired_owner_statement() {
        let transfer = read_statement(RD_ATTACHMENT_OPERATIONAL_TRANSFER).expect("read kind");
        assert_eq!(
            transfer.sql,
            "SELECT company_id,source_branch_id,target_branch_id,NULL::text department_id,version,status,created_by,prepared_by,delivery_mode FROM inventory_transfers WHERE id=? AND company_id=?"
        );
        assert_eq!(transfer.bind_count, 2);
        assert!(transfer.sql.trim_start().starts_with("SELECT"));
        assert_eq!(transfer.sql.matches('?').count(), transfer.bind_count);
    }

    /// Parity witness for the owner-local `attachments-policy-facts` transit
    /// descriptor read: the native plan is the exact statement the retired
    /// TypeScript literal bound on the inventory owner's `inventory_transit`
    /// table, with the caller-unit binds intact and no cross-owner projection.
    #[test]
    fn transit_attachment_descriptor_read_plan_matches_retired_owner_statement() {
        let transit = read_statement(RD_TRANSIT_ATTACHMENT_DESCRIPTOR).expect("read kind");
        assert_eq!(
            transit.sql,
            "SELECT company_id,source_branch_id,target_branch_id,version FROM inventory_transit WHERE id=? AND company_id=?"
        );
        assert_eq!(transit.bind_count, 2);
        assert!(transit.sql.trim_start().starts_with("SELECT"));
        assert_eq!(transit.sql.matches('?').count(), transit.bind_count);
    }

    /// Parity witness for the `attachments-policy-facts` descriptor family:
    /// each fixed plan is exactly the statement the retired TypeScript template
    /// resolved for that target type (constant branch/version columns, NULL
    /// department and empty extra), with the caller-unit binds intact.
    #[test]
    fn attachment_descriptor_read_plans_match_retired_owner_statements() {
        let cases = [
            (
                RD_ATTACHMENT_DESCRIPTOR_LANDED_COST,
                "SELECT company_id,branch_id AS branch_id,NULL AS department_id,version AS version FROM inventory_landed_costs WHERE id=? AND company_id=?",
            ),
            (
                RD_ATTACHMENT_DESCRIPTOR_STOCK_COUNT,
                "SELECT company_id,branch_id AS branch_id,NULL AS department_id,version AS version FROM inventory_counts WHERE id=? AND company_id=?",
            ),
            (
                RD_ATTACHMENT_DESCRIPTOR_TRANSFER,
                "SELECT company_id,source_branch_id AS branch_id,NULL AS department_id,version AS version FROM inventory_transfers WHERE id=? AND company_id=?",
            ),
            (
                RD_ATTACHMENT_DESCRIPTOR_TRANSIT,
                "SELECT company_id,source_branch_id AS branch_id,NULL AS department_id,version AS version FROM inventory_transit WHERE id=? AND company_id=?",
            ),
            (
                RD_ATTACHMENT_DESCRIPTOR_LOT,
                "SELECT company_id,NULL AS branch_id,NULL AS department_id,version AS version FROM inventory_lots WHERE id=? AND company_id=?",
            ),
        ];
        for (kind, expected) in cases {
            let read = read_statement(kind).expect("read kind");
            assert_eq!(read.sql, expected, "{kind}");
            assert_eq!(read.bind_count, 2, "{kind}");
            assert_eq!(read.sql.matches('?').count(), read.bind_count, "{kind}");
            assert_eq!(read.scope, "holding", "{kind}");
        }
    }

    /// Parity witness for the owner-local `commerce-attachment-sources` stock
    /// image: the native plan aggregates only Inventory's own reservation,
    /// allocation, lot and lot-balance tables with the retired literal's four
    /// caller-unit binds and `image` column alias.
    #[test]
    fn commerce_attachment_stock_image_plan_matches_retired_owner_statement() {
        let read = read_statement(RD_COMMERCE_ATTACHMENT_STOCK_IMAGE).expect("read kind");
        assert_eq!(read.bind_count, 4);
        assert_eq!(read.sql.matches('?').count(), read.bind_count);
        assert_eq!(read.scope, "holding");
        assert!(read.sql.starts_with("WITH allocations AS MATERIALIZED(SELECT a.* FROM inventory_fulfillment_lot_allocations a"));
        assert!(read.sql.contains("lots AS MATERIALIZED(SELECT l.* FROM inventory_lots l WHERE l.company_id=? AND(l.id IN(SELECT lot_id FROM allocations)"));
        assert!(read.sql.contains("FROM inventory_order_reservations r WHERE r.order_line_id IN(SELECT jsonb_array_elements_text(?::jsonb))"));
        assert!(read.sql.contains("FROM inventory_lot_balances b JOIN lots l ON l.id=b.lot_id))::text image"));
        assert!(!read.sql.contains("commerce_"));
    }

    /// Parity witness for the owner-local `commerce-facts` in-transit totals
    /// read: the native plan is the exact statement the retired TypeScript
    /// literal bound on Inventory's `inventory_in_transit` table, with the same
    /// four caller-unit binds and full array result shape.
    #[test]
    fn in_transit_currency_totals_plan_matches_retired_owner_statement() {
        let read = read_statement(RD_IN_TRANSIT_CURRENCY_TOTALS).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT currency,count(*) line_count,SUM(value_minor) value_minor FROM inventory_in_transit WHERE company_id=? AND quantity_milli>0 AND (?::text IS NULL OR source_branch_id=? OR target_branch_id=?) GROUP BY currency ORDER BY currency"
        );
        assert_eq!(read.bind_count, 4);
        assert_eq!(read.sql.matches('?').count(), read.bind_count);
        assert_eq!(read.scope, "holding");
    }

    /// Parity witness for the owner-local `commerce-facts` fulfillment lot
    /// allocation read: the native plan is the exact statement the retired
    /// TypeScript literal bound on Inventory's own allocation and lot tables,
    /// with the caller-unit binds intact and no cross-owner projection.
    #[test]
    fn fulfillment_lot_allocation_plan_matches_retired_owner_statement() {
        let read = read_statement(RD_FULFILLMENT_LOT_ALLOCATION).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT a.*,l.tracking_mode FROM inventory_fulfillment_lot_allocations a JOIN inventory_lots l ON l.id=a.lot_id WHERE a.fulfillment_line_id=? AND a.lot_id=?"
        );
        assert_eq!(read.bind_count, 2);
        assert_eq!(read.sql.matches('?').count(), read.bind_count);
        assert_eq!(read.scope, "holding");
    }

    /// Parity witness: the native plan for the lot-balance supplement is the
    /// exact statement the retired TypeScript `readRecordSupplementDataRows`
    /// bound (with the commerce warehouse projection inlined). The owner owns
    /// this SQL, so the TS adapter emits none of it.
    #[test]
    fn lot_balance_read_plan_matches_retired_owner_statement() {
        let read = read_statement(RD_LOT_BALANCE_SUPPLEMENT_ROWS).expect("read kind");
        assert_eq!(read.bind_count, 5);
        assert_eq!(read.sql.matches('?').count(), read.bind_count);
        assert_eq!(
            read.sql,
            r#"SELECT b.*,w.name warehouse_name FROM inventory_lot_balances b JOIN (SELECT "id","company_id","branch_id","code","name","status","version" FROM commerce_warehouses) w ON w.id=b.warehouse_id WHERE b.lot_id=? AND w.company_id=? AND (?::text IS NULL OR w.branch_id=?) ORDER BY w.name LIMIT 31 OFFSET ?"#
        );
    }
}
