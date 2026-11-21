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
}
