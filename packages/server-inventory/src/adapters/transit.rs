//! PostgreSQL transport source/effect seam for the native Transit command.
//!
//! The caller (composition) owns current IAM/Commercial authority, the Workflow
//! command ledger, the Finance journal and Files evidence. These functions read
//! current owner facts, revalidate the planned outcome and apply only the
//! Inventory stock/lot/event effects inside the caller's serializable unit.
use crate::domain::transit::{
    TransitCancelPlan, TransitDispatchPlan, TransitEvent, TransitLine, TransitLot, TransitRecord,
    TransitSource,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn invalid(code: &'static str, message: &'static str) -> Error {
    BusinessError::invalid(code, message).into()
}

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "TRANSIT_SOURCE_CHANGED",
        "The transport record changed. Reload its current state before submitting.",
    )
    .into()
}

async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("INVENTORY_OWNER_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("INVENTORY_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

/// Lock every relation the transit command reads or writes before the first
/// owner read. A concurrent transfer/transit writer waits for this unit.
pub async fn lock_transit_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        "LOCK TABLE inventory_transit,inventory_transit_lines,inventory_transit_events,\
         inventory_transit_lots,inventory_transit_lot_events,inventory_transfers,\
         inventory_transfer_lines,commerce_stock,inventory_lot_balances,inventory_lot_moves \
         IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(&mut *c)
    .await?;
    Ok(())
}

/// Read the current transit source for the selected Company scope.
pub async fn read_transit_source(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    id: &str,
) -> Result<TransitSource, Error> {
    serializable(c).await?;
    if company.is_empty() || id.is_empty() {
        return Err(invalid("TRANSIT_NOT_FOUND", "Transport record not found."));
    }
    let transit = sqlx::query(
        "SELECT d.id,d.transfer_id,d.company_id,d.source_warehouse_id,d.target_warehouse_id,\
         d.source_branch_id,d.target_branch_id,d.status,d.due_date,d.currency,d.chart_id,\
         d.inventory_account_id,d.created_by,d.approved_by,d.dispatched_by,d.dispatch_date,\
         d.tracking_reference,d.version,d.created_at,d.updated_at,t.number,t.transfer_date \
         FROM inventory_transit d JOIN inventory_transfers t ON t.id=d.transfer_id \
         WHERE d.id=$1 AND d.company_id=$2 AND ($3::text IS NULL OR d.source_branch_id=$3 OR d.target_branch_id=$3)",
    )
    .bind(id)
    .bind(company)
    .bind(branch)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| {
        BusinessError::new(
            FailureKind::NotFound,
            "TRANSIT_NOT_FOUND",
            "Transport record not found in this scope.",
        )
    })?;
    let lines = sqlx::query(
        "SELECT l.id,l.product_id,p.tracking_mode,l.quantity_milli,l.value_minor,\
         l.received_milli,l.lost_milli,l.settled_value_minor,\
         COALESCE((SELECT SUM(e.quantity_milli)::bigint FROM inventory_transit_events e \
           WHERE e.line_id=l.id AND e.kind='loss_requested' \
             AND NOT EXISTS(SELECT 1 FROM inventory_transit_events r WHERE r.proposal_id=e.id)),0) pending_loss_milli,\
         s.source_version,s.source_quantity_milli,s.source_value_minor,s.product_version \
         FROM inventory_transit_lines l \
         JOIN commerce_products p ON p.id=l.product_id \
         JOIN inventory_transfer_lines s ON s.id=l.source_line_id \
         WHERE l.transit_id=$1 ORDER BY l.id",
    )
    .bind(id)
    .fetch_all(&mut *c)
    .await?;
    let lots = sqlx::query(
        "SELECT tl.id,tl.transit_line_id,tl.lot_id,tl.quantity_milli,tl.value_minor,\
         tl.received_milli,tl.lost_milli,tl.settled_value_minor,\
         COALESCE((SELECT SUM(le.quantity_milli)::bigint FROM inventory_transit_lot_events le \
           JOIN inventory_transit_events e ON e.id=le.event_id \
           WHERE le.transit_lot_id=tl.id AND e.kind='loss_requested' \
             AND NOT EXISTS(SELECT 1 FROM inventory_transit_events r WHERE r.proposal_id=e.id)),0) pending_loss_milli \
         FROM inventory_transit_lots tl \
         JOIN inventory_transit_lines line ON line.id=tl.transit_line_id \
         WHERE line.transit_id=$1 ORDER BY line.id,tl.id",
    )
    .bind(id)
    .fetch_all(&mut *c)
    .await?;
    let events = sqlx::query(
        "SELECT e.id,e.line_id,e.kind,e.quantity_milli,e.movement_date,e.actor_id,e.proposal_id,\
         EXISTS(SELECT 1 FROM inventory_transit_events r WHERE r.proposal_id=e.id) resolved \
         FROM inventory_transit_events e WHERE e.transit_id=$1 ORDER BY e.created_at,e.id",
    )
    .bind(id)
    .fetch_all(&mut *c)
    .await?;
    Ok(TransitSource {
        transit: TransitRecord {
            id: transit.try_get("id")?,
            transfer_id: transit.try_get("transfer_id")?,
            company_id: transit.try_get("company_id")?,
            source_warehouse_id: transit.try_get("source_warehouse_id")?,
            target_warehouse_id: transit.try_get("target_warehouse_id")?,
            source_branch_id: transit.try_get("source_branch_id")?,
            target_branch_id: transit.try_get("target_branch_id")?,
            status: transit.try_get("status")?,
            due_date: transit.try_get("due_date")?,
            currency: transit.try_get("currency")?,
            chart_id: transit.try_get("chart_id")?,
            inventory_account_id: transit.try_get("inventory_account_id")?,
            created_by: transit.try_get("created_by")?,
            approved_by: transit.try_get("approved_by")?,
            dispatched_by: transit.try_get("dispatched_by")?,
            dispatch_date: transit.try_get("dispatch_date")?,
            tracking_reference: transit.try_get("tracking_reference")?,
            version: transit.try_get("version")?,
            number: transit.try_get("number")?,
            transfer_date: transit.try_get("transfer_date")?,
        },
        lines: lines
            .into_iter()
            .map(|row| {
                Ok(TransitLine {
                    id: row.try_get("id")?,
                    product_id: row.try_get("product_id")?,
                    tracking_mode: row.try_get("tracking_mode")?,
                    quantity_milli: row.try_get("quantity_milli")?,
                    value_minor: row.try_get("value_minor")?,
                    received_milli: row.try_get("received_milli")?,
                    lost_milli: row.try_get("lost_milli")?,
                    settled_value_minor: row.try_get("settled_value_minor")?,
                    pending_loss_milli: row.try_get("pending_loss_milli")?,
                    source_version: row.try_get("source_version")?,
                    source_quantity_milli: row.try_get("source_quantity_milli")?,
                    source_value_minor: row.try_get("source_value_minor")?,
                    product_version: row.try_get("product_version")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?,
        lots: lots
            .into_iter()
            .map(|row| {
                Ok(TransitLot {
                    id: row.try_get("id")?,
                    transit_line_id: row.try_get("transit_line_id")?,
                    lot_id: row.try_get("lot_id")?,
                    quantity_milli: row.try_get("quantity_milli")?,
                    value_minor: row.try_get("value_minor")?,
                    received_milli: row.try_get("received_milli")?,
                    lost_milli: row.try_get("lost_milli")?,
                    settled_value_minor: row.try_get("settled_value_minor")?,
                    pending_loss_milli: row.try_get("pending_loss_milli")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?,
        events: events
            .into_iter()
            .map(|row| {
                Ok(TransitEvent {
                    id: row.try_get("id")?,
                    line_id: row.try_get("line_id")?,
                    kind: row.try_get("kind")?,
                    quantity_milli: row.try_get("quantity_milli")?,
                    movement_date: row.try_get("movement_date")?,
                    actor_id: row.try_get("actor_id")?,
                    proposal_id: row.try_get("proposal_id")?,
                    resolved: row.try_get("resolved")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?,
    })
}

/// Read the pending proposal's selected transport lots for the Finance
/// decision. The returned rows are the current owner evidence, not authority.
pub async fn read_proposed_loss_lots(
    c: &mut PgConnection,
    proposal_id: &str,
) -> Result<Vec<crate::domain::transit::TransitProposedLossLot>, Error> {
    let rows = sqlx::query(
        "SELECT le.transit_lot_id,le.quantity_milli,tl.quantity_milli total_quantity_milli,\
         tl.value_minor total_value_minor,tl.received_milli,tl.lost_milli,tl.settled_value_minor \
         FROM inventory_transit_lot_events le \
         JOIN inventory_transit_lots tl ON tl.id=le.transit_lot_id \
         WHERE le.event_id=$1 ORDER BY le.id",
    )
    .bind(proposal_id)
    .fetch_all(&mut *c)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(crate::domain::transit::TransitProposedLossLot {
                transit_lot_id: row.try_get("transit_lot_id")?,
                quantity_milli: row.try_get("quantity_milli")?,
                total_quantity_milli: row.try_get("total_quantity_milli")?,
                total_value_minor: row.try_get("total_value_minor")?,
                received_milli: row.try_get("received_milli")?,
                lost_milli: row.try_get("lost_milli")?,
                settled_value_minor: row.try_get("settled_value_minor")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(Into::into)
}

async fn guard_not_before_movement(
    c: &mut PgConnection,
    warehouse: &str,
    product: &str,
    date: &str,
    code: &'static str,
) -> Result<(), Error> {
    let later: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM inventory_ledger WHERE warehouse_id=$1 AND product_id=$2 AND movement_date>$3)",
    )
    .bind(warehouse)
    .bind(product)
    .bind(date)
    .fetch_one(&mut *c)
    .await?;
    if later {
        return Err(invalid(
            code,
            "The movement date precedes later stock activity.",
        ));
    }
    Ok(())
}

async fn insert_event(
    c: &mut PgConnection,
    id: &str,
    transit_id: &str,
    line_id: Option<&str>,
    kind: &str,
    quantity_milli: i64,
    value_minor: i64,
    date: &str,
    evidence: &str,
    actor: &str,
    reviewed_version: i64,
    proposal_id: Option<&str>,
    now: i64,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO inventory_transit_events(id,transit_id,line_id,kind,quantity_milli,value_minor,\
         movement_date,evidence,actor_id,reviewed_version,proposal_id,created_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
    )
    .bind(id)
    .bind(transit_id)
    .bind(line_id)
    .bind(kind)
    .bind(quantity_milli)
    .bind(value_minor)
    .bind(date)
    .bind(evidence)
    .bind(actor)
    .bind(reviewed_version)
    .bind(proposal_id)
    .bind(now)
    .execute(&mut *c)
    .await?;
    Ok(())
}

/// Apply an approved dispatch. The plan was produced by `plan_dispatch` from
/// the same source and is revalidated against current rows before writes.
pub async fn apply_transit_dispatch(
    c: &mut PgConnection,
    transit_id: &str,
    plan: &TransitDispatchPlan,
    command_key: &str,
    actor: &str,
    now: i64,
) -> Result<(), Error> {
    serializable(c).await?;
    if command_key.is_empty() || actor.is_empty() || plan.lines.is_empty() {
        return Err(invalid(
            "TRANSIT_COMMAND_INVALID",
            "Use a current transport dispatch command.",
        ));
    }
    let transit = sqlx::query(
        "SELECT d.id,d.company_id,d.source_warehouse_id,d.target_warehouse_id,d.status,d.version,\
         d.transfer_id,d.dispatch_date FROM inventory_transit d WHERE d.id=$1",
    )
    .bind(transit_id)
    .fetch_optional(&mut *c)
    .await?;
    let Some(transit) = transit else {
        return Err(changed());
    };
    let source_warehouse: String = transit.try_get("source_warehouse_id")?;
    let company: String = transit.try_get("company_id")?;
    let status: String = transit.try_get("status")?;
    let version: i64 = transit.try_get("version")?;
    if status != "awaiting_dispatch" || version + 1 != plan.version {
        return Err(changed());
    }
    for (index, line) in plan.lines.iter().enumerate() {
        guard_not_before_movement(
            c,
            &source_warehouse,
            &line.product_id,
            &plan.date,
            "DISPATCH_DATE_INVALID",
        )
        .await?;
        let product: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM commerce_products WHERE id=$1 AND company_id=$2 AND version=$3)",
        )
        .bind(&line.product_id)
        .bind(&company)
        .bind(line.product_version)
        .fetch_one(&mut *c)
        .await?;
        if !product {
            return Err(changed());
        }
        let stock = sqlx::query(
            "UPDATE commerce_stock SET quantity_milli=quantity_milli-$1,value_minor=value_minor-$2,\
             version=version+1 WHERE warehouse_id=$3 AND product_id=$4 AND quantity_milli=$5 \
             AND value_minor=$6 AND version=$7",
        )
        .bind(line.quantity_milli)
        .bind(line.value_minor)
        .bind(&source_warehouse)
        .bind(&line.product_id)
        .bind(line.source_quantity_milli)
        .bind(line.source_value_minor)
        .bind(line.source_version)
        .execute(&mut *c)
        .await?;
        if stock.rows_affected() != 1 {
            return Err(changed());
        }
        let event_id = format!("{command_key}-{index}");
        insert_event(
            c,
            &event_id,
            transit_id,
            Some(&line.line_id),
            "dispatch",
            line.quantity_milli,
            line.value_minor,
            &plan.date,
            &plan.evidence,
            actor,
            plan.version - 1,
            None,
            now,
        )
        .await?;
        for (lot_index, lot) in line.lots.iter().enumerate() {
            let moved = sqlx::query(
                "UPDATE inventory_lot_balances SET quantity_milli=quantity_milli-$1,\
                 value_minor=value_minor-$2,version=version+1 \
                 WHERE lot_id=$3 AND warehouse_id=$4 AND quantity_milli>=$1 AND value_minor>=$2",
            )
            .bind(lot.quantity_milli)
            .bind(lot.value_minor)
            .bind(&lot.lot_id)
            .bind(&source_warehouse)
            .execute(&mut *c)
            .await?;
            if moved.rows_affected() != 1 {
                return Err(changed());
            }
            sqlx::query(
                "INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,source_id,\
                 quantity_milli,value_minor,movement_date,created_by,created_at) \
                 VALUES($1,$2,$3,'transit',$4,$5,$6,$7,$8,$9)",
            )
            .bind(format!("lot-move-{event_id}-{lot_index}"))
            .bind(&lot.lot_id)
            .bind(&source_warehouse)
            .bind(&event_id)
            .bind(-lot.quantity_milli)
            .bind(-lot.value_minor)
            .bind(&plan.date)
            .bind(actor)
            .bind(now)
            .execute(&mut *c)
            .await?;
            sqlx::query(
                "INSERT INTO inventory_transit_lot_events(id,event_id,transit_lot_id,quantity_milli,value_minor) \
                 VALUES($1,$2,$3,$4,$5)",
            )
            .bind(format!("transit-lot-event-{event_id}-{lot_index}"))
            .bind(&event_id)
            .bind(&lot.transit_lot_id)
            .bind(lot.quantity_milli)
            .bind(lot.value_minor)
            .execute(&mut *c)
            .await?;
        }
    }
    let updated = sqlx::query(
        "UPDATE inventory_transit SET status='in_transit',dispatch_date=$1,dispatched_by=$2,\
         tracking_reference=$3,version=version+1,updated_at=$4 WHERE id=$5 AND version=$6 \
         AND status='awaiting_dispatch'",
    )
    .bind(&plan.date)
    .bind(actor)
    .bind(&plan.tracking_reference)
    .bind(now)
    .bind(transit_id)
    .bind(plan.version - 1)
    .execute(&mut *c)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(changed());
    }
    Ok(())
}

/// Apply a pre-dispatch cancellation and its terminal audit event.
pub async fn apply_transit_cancel(
    c: &mut PgConnection,
    transit_id: &str,
    plan: &TransitCancelPlan,
    command_key: &str,
    actor: &str,
    now: i64,
) -> Result<(), Error> {
    serializable(c).await?;
    if transit_id.is_empty() || command_key.is_empty() || actor.is_empty() {
        return Err(invalid(
            "TRANSIT_COMMAND_INVALID",
            "Use a current transport cancellation command.",
        ));
    }
    let updated = sqlx::query(
        "UPDATE inventory_transit SET status='cancelled',version=version+1,updated_at=$1 \
         WHERE id=$2 AND version=$3 AND status='awaiting_dispatch'",
    )
    .bind(now)
    .bind(transit_id)
    .bind(plan.version - 1)
    .execute(&mut *c)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(changed());
    }
    insert_event(
        c,
        &format!("cancel-{command_key}"),
        transit_id,
        None,
        "cancel",
        0,
        0,
        &chrono_date(now),
        &plan.reason,
        actor,
        plan.version - 1,
        None,
        now,
    )
    .await
}

fn chrono_date(now: i64) -> String {
    // Commands always supply an explicit movement date; cancellation has none,
    // so the retained writer used the current UTC calendar date.
    let seconds = now.div_euclid(1_000);
    let days = seconds.div_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + i64::from(m <= 2), m, d)
}

const MAX_SAFE: i64 = 9_007_199_254_740_991;

async fn settle_transit(
    c: &mut PgConnection,
    transit_id: &str,
    expected_version: i64,
    now: i64,
) -> Result<(), Error> {
    let updated = sqlx::query(
        "UPDATE inventory_transit SET \
         status=CASE WHEN NOT EXISTS(SELECT 1 FROM inventory_transit_lines \
           WHERE transit_id=$1 AND received_milli+lost_milli<quantity_milli) \
           THEN 'completed' ELSE status END,\
         version=version+1,updated_at=$2 WHERE id=$1 AND version=$3",
    )
    .bind(transit_id)
    .bind(now)
    .bind(expected_version)
    .execute(&mut *c)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(changed());
    }
    Ok(())
}

/// Apply a received outcome. The target stock, transit line and every selected
/// transit lot are advanced together inside the caller's transaction.
pub async fn apply_transit_receive(
    c: &mut PgConnection,
    transit_id: &str,
    plan: &crate::domain::transit::TransitReceivePlan,
    command_key: &str,
    actor: &str,
    now: i64,
) -> Result<(), Error> {
    serializable(c).await?;
    if command_key.is_empty() || actor.is_empty() || plan.lines.is_empty() {
        return Err(invalid(
            "TRANSIT_COMMAND_INVALID",
            "Use a current transport receipt command.",
        ));
    }
    let transit = sqlx::query(
        "SELECT target_warehouse_id,status,version FROM inventory_transit WHERE id=$1",
    )
    .bind(transit_id)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(changed)?;
    let target: String = transit.try_get("target_warehouse_id")?;
    let status: String = transit.try_get("status")?;
    let version: i64 = transit.try_get("version")?;
    if status != "in_transit" || version + 1 != plan.version {
        return Err(changed());
    }
    for (index, line) in plan.lines.iter().enumerate() {
        guard_not_before_movement(
            c,
            &target,
            &line.product_id,
            &plan.date,
            "RECEIPT_DATE_INVALID",
        )
        .await?;
        let event_id = format!("{command_key}-{index}");
        let stock = sqlx::query(
            "SELECT quantity_milli,value_minor,version FROM commerce_stock \
             WHERE warehouse_id=$1 AND product_id=$2",
        )
        .bind(&target)
        .bind(&line.product_id)
        .fetch_optional(&mut *c)
        .await?;
        match stock {
            Some(stock) => {
                let quantity: i64 = stock.try_get("quantity_milli")?;
                let value: i64 = stock.try_get("value_minor")?;
                let stock_version: i64 = stock.try_get("version")?;
                let next_quantity = quantity.checked_add(line.quantity_milli);
                let next_value = value.checked_add(line.value_minor);
                if next_quantity.is_none_or(|v| v > MAX_SAFE)
                    || next_value.is_none_or(|v| v > MAX_SAFE)
                {
                    return Err(invalid(
                        "VALUE_OVERFLOW",
                        "Receiving stock exceeds supported precision.",
                    ));
                }
                let updated = sqlx::query(
                    "UPDATE commerce_stock SET quantity_milli=quantity_milli+$1,\
                     value_minor=value_minor+$2,version=version+1 \
                     WHERE warehouse_id=$3 AND product_id=$4 AND version=$5",
                )
                .bind(line.quantity_milli)
                .bind(line.value_minor)
                .bind(&target)
                .bind(&line.product_id)
                .bind(stock_version)
                .execute(&mut *c)
                .await?;
                if updated.rows_affected() != 1 {
                    return Err(changed());
                }
            }
            None => {
                sqlx::query(
                    "INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) \
                     VALUES($1,$2,$3,$4)",
                )
                .bind(&target)
                .bind(&line.product_id)
                .bind(line.quantity_milli)
                .bind(line.value_minor)
                .execute(&mut *c)
                .await?;
            }
        }
        let updated = sqlx::query(
            "UPDATE inventory_transit_lines SET received_milli=received_milli+$1,\
             settled_value_minor=settled_value_minor+$2 WHERE id=$3",
        )
        .bind(line.quantity_milli)
        .bind(line.value_minor)
        .bind(&line.line_id)
        .execute(&mut *c)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(changed());
        }
        insert_event(
            c,
            &event_id,
            transit_id,
            Some(&line.line_id),
            "receipt",
            line.quantity_milli,
            line.value_minor,
            &plan.date,
            &plan.evidence,
            actor,
            plan.version - 1,
            None,
            now,
        )
        .await?;
        for (lot_index, lot) in line.lots.iter().enumerate() {
            let balance = sqlx::query(
                "SELECT version FROM inventory_lot_balances WHERE lot_id=$1 AND warehouse_id=$2",
            )
            .bind(&lot.lot_id)
            .bind(&target)
            .fetch_optional(&mut *c)
            .await?;
            match balance {
                Some(balance) => {
                    let lot_version: i64 = balance.try_get("version")?;
                    let updated = sqlx::query(
                        "UPDATE inventory_lot_balances SET quantity_milli=quantity_milli+$1,\
                         value_minor=value_minor+$2,version=version+1 \
                         WHERE lot_id=$3 AND warehouse_id=$4 AND version=$5",
                    )
                    .bind(lot.quantity_milli)
                    .bind(lot.value_minor)
                    .bind(&lot.lot_id)
                    .bind(&target)
                    .bind(lot_version)
                    .execute(&mut *c)
                    .await?;
                    if updated.rows_affected() != 1 {
                        return Err(changed());
                    }
                }
                None => {
                    sqlx::query(
                        "INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor) \
                         VALUES($1,$2,$3,$4)",
                    )
                    .bind(&lot.lot_id)
                    .bind(&target)
                    .bind(lot.quantity_milli)
                    .bind(lot.value_minor)
                    .execute(&mut *c)
                    .await?;
                }
            }
            sqlx::query(
                "INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,source_id,\
                 quantity_milli,value_minor,movement_date,created_by,created_at) \
                 VALUES($1,$2,$3,'transit',$4,$5,$6,$7,$8,$9)",
            )
            .bind(format!("lot-move-{event_id}-{lot_index}"))
            .bind(&lot.lot_id)
            .bind(&target)
            .bind(&event_id)
            .bind(lot.quantity_milli)
            .bind(lot.value_minor)
            .bind(&plan.date)
            .bind(actor)
            .bind(now)
            .execute(&mut *c)
            .await?;
            sqlx::query(
                "INSERT INTO inventory_transit_lot_events(id,event_id,transit_lot_id,quantity_milli,value_minor) \
                 VALUES($1,$2,$3,$4,$5)",
            )
            .bind(format!("transit-lot-event-{event_id}-{lot_index}"))
            .bind(&event_id)
            .bind(&lot.transit_lot_id)
            .bind(lot.quantity_milli)
            .bind(lot.value_minor)
            .execute(&mut *c)
            .await?;
            let updated = sqlx::query(
                "UPDATE inventory_transit_lots SET received_milli=received_milli+$1,\
                 settled_value_minor=settled_value_minor+$2 WHERE id=$3",
            )
            .bind(lot.quantity_milli)
            .bind(lot.value_minor)
            .bind(&lot.transit_lot_id)
            .execute(&mut *c)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(changed());
            }
        }
    }
    settle_transit(c, transit_id, plan.version - 1, now).await
}

/// Apply a loss proposal. Quantity is reserved by the proposal event and its
/// selected lot events; value remains Finance-owned until the decision.
pub async fn apply_transit_loss_request(
    c: &mut PgConnection,
    transit_id: &str,
    plan: &crate::domain::transit::TransitLossPlan,
    command_key: &str,
    actor: &str,
    now: i64,
) -> Result<(), Error> {
    serializable(c).await?;
    if command_key.is_empty() || actor.is_empty() || plan.quantity_milli <= 0 {
        return Err(invalid(
            "TRANSIT_COMMAND_INVALID",
            "Use a current transport loss command.",
        ));
    }
    let transit =
        sqlx::query("SELECT status,version FROM inventory_transit WHERE id=$1")
            .bind(transit_id)
            .fetch_optional(&mut *c)
            .await?
            .ok_or_else(changed)?;
    let status: String = transit.try_get("status")?;
    let version: i64 = transit.try_get("version")?;
    if status != "in_transit" || version + 1 != plan.version {
        return Err(changed());
    }
    let proposal_id = format!("loss-{command_key}");
    insert_event(
        c,
        &proposal_id,
        transit_id,
        Some(&plan.line_id),
        "loss_requested",
        plan.quantity_milli,
        0,
        &plan.date,
        &plan.evidence,
        actor,
        plan.version - 1,
        None,
        now,
    )
    .await?;
    for (index, lot) in plan.lots.iter().enumerate() {
        sqlx::query(
            "INSERT INTO inventory_transit_lot_events(id,event_id,transit_lot_id,quantity_milli,value_minor) \
             VALUES($1,$2,$3,$4,0)",
        )
        .bind(format!("transit-lot-event-{proposal_id}-{index}"))
        .bind(&proposal_id)
        .bind(&lot.transit_lot_id)
        .bind(lot.quantity_milli)
        .execute(&mut *c)
        .await?;
    }
    settle_transit(c, transit_id, plan.version - 1, now).await
}

/// Apply a Finance loss decision. An approval settles stock value against the
/// caller-supplied journal; a rejection only closes the proposal.
pub async fn apply_transit_loss_decision(
    c: &mut PgConnection,
    transit_id: &str,
    plan: &crate::domain::transit::TransitLossDecisionPlan,
    command_key: &str,
    actor: &str,
    journal_id: Option<&str>,
    now: i64,
) -> Result<(), Error> {
    serializable(c).await?;
    if command_key.is_empty() || actor.is_empty() {
        return Err(invalid(
            "TRANSIT_COMMAND_INVALID",
            "Use a current transport loss decision.",
        ));
    }
    let transit =
        sqlx::query("SELECT status,version FROM inventory_transit WHERE id=$1")
            .bind(transit_id)
            .fetch_optional(&mut *c)
            .await?
            .ok_or_else(changed)?;
    let status: String = transit.try_get("status")?;
    let version: i64 = transit.try_get("version")?;
    if status != "in_transit" || version + 1 != plan.version {
        return Err(changed());
    }
    let pending: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM inventory_transit_events WHERE proposal_id=$1)",
    )
    .bind(&plan.proposal_id)
    .fetch_one(&mut *c)
    .await?;
    if pending {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "TRANSIT_LOSS_ALREADY_DECIDED",
            "This transport discrepancy was already decided.",
        )
        .into());
    }
    if plan.approved {
        let updated = sqlx::query(
            "UPDATE inventory_transit_lines SET lost_milli=lost_milli+$1,\
             settled_value_minor=settled_value_minor+$2 WHERE id=$3",
        )
        .bind(plan.quantity_milli)
        .bind(plan.value_minor)
        .bind(&plan.line_id)
        .execute(&mut *c)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(changed());
        }
        for lot in &plan.lots {
            let updated = sqlx::query(
                "UPDATE inventory_transit_lots SET lost_milli=lost_milli+$1,\
                 settled_value_minor=settled_value_minor+$2 WHERE id=$3",
            )
            .bind(lot.quantity_milli)
            .bind(lot.value_minor)
            .bind(&lot.transit_lot_id)
            .execute(&mut *c)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(changed());
            }
        }
    }
    sqlx::query(
        "INSERT INTO inventory_transit_events(id,transit_id,line_id,kind,quantity_milli,value_minor,\
         movement_date,evidence,actor_id,reviewed_version,proposal_id,journal_id,created_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
    )
    .bind(format!("decision-{command_key}"))
    .bind(transit_id)
    .bind(&plan.line_id)
    .bind(if plan.approved {
        "loss_approved"
    } else {
        "loss_rejected"
    })
    .bind(plan.quantity_milli)
    .bind(plan.value_minor)
    .bind(&plan.date)
    .bind(&plan.evidence)
    .bind(actor)
    .bind(plan.version - 1)
    .bind(&plan.proposal_id)
    .bind(journal_id)
    .bind(now)
    .execute(&mut *c)
    .await?;
    settle_transit(c, transit_id, plan.version - 1, now).await
}
