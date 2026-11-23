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

pub fn is_read(kind: &str) -> bool {
    matches!(
        kind,
        RD_LEDGER_SUPPLEMENT_ROWS
            | RD_LOT_BALANCE_SUPPLEMENT_ROWS
            | RD_LEDGER_SUPPLEMENT_ROW
            | RD_LOT_SUPPLEMENT_ROW
            | RD_STOCK_SUPPLEMENT_ROW
    )
}

pub fn read_statement(kind: &str) -> Option<&'static NativeStatement> {
    match kind {
        RD_LEDGER_SUPPLEMENT_ROWS => Some(&READ_LEDGER_SUPPLEMENT),
        RD_LOT_BALANCE_SUPPLEMENT_ROWS => Some(&READ_LOT_BALANCE_SUPPLEMENT),
        RD_LEDGER_SUPPLEMENT_ROW => Some(&READ_LEDGER_SUPPLEMENT_ROW),
        RD_LOT_SUPPLEMENT_ROW => Some(&READ_LOT_SUPPLEMENT_ROW),
        RD_STOCK_SUPPLEMENT_ROW => Some(&READ_STOCK_SUPPLEMENT_ROW),
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
        ] {
            assert!(is_read(kind));
            let read = read_statement(kind).expect("read kind");
            assert!(read.sql.trim_start().starts_with("SELECT"), "{kind}");
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
