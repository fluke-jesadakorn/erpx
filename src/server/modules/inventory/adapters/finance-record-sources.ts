import {serviceInitialPhysicalStockCountSourceRows,serviceInitialPhysicalStockMarkedRows,serviceInitialPhysicalStockSourceImages} from './service-initial-physical-stock-count-source-sql';
import {posInitialPhysicalStockCountSourceRows,posInitialPhysicalStockMarkedRows,posInitialPhysicalStockSourceImages} from './pos-initial-physical-stock-count-source-sql';
import {supplyInventoryTerminalReceipts} from './supply-terminal-receipts';
import {manufacturingInitialPhysicalStockCountSourceRows,manufacturingInitialPhysicalStockMarkedRows,manufacturingInitialPhysicalStockSourceImages} from './manufacturing-initial-physical-stock-count-source-sql';
import {initialPhysicalStockCountSourceRows,initialPhysicalStockMarkedRows} from './initial-physical-stock-count-source-sql';
import {packageClockSql,purchasedCapabilityPredicate} from "../../commercial/public/purchased-capability-sql";
import "server-only";
import {landedCostPurchaseSourceRows,landedCostSourceReadLocks} from "./landed-cost-purchases";
import {qualityDisposalCountRows} from "./quality-count-purchases";
import {qualityPurchaseSourceReadLocks} from "../../quality/public/purchase-sources";
import type {SqlDatabase} from "../../runtime/public/runtime/types";

/** Exact posted physical-count ownership. Count commands own independent
 * stock/GL posting; keeping this alias preserves its established history port. */
const countJournal=`SELECT c.company_id,c.journal_id id FROM inventory_counts c JOIN finance_journals j ON j.id=c.journal_id AND j.company_id=c.company_id AND j.source_type='stock_count' AND j.source_id=c.id AND j.status='posted' WHERE c.status='posted' AND NOT EXISTS(SELECT 1 FROM (${initialPhysicalStockMarkedRows} UNION ALL ${manufacturingInitialPhysicalStockMarkedRows} UNION ALL ${posInitialPhysicalStockMarkedRows} UNION ALL ${serviceInitialPhysicalStockMarkedRows}) marker WHERE marker.id=c.id)`;
/** A source label cannot establish loss ownership. The immutable approval,
 * original proposal/line and retained same-Company transport/approved transfer
 * must all point at the exact posted journal, chart, currency and loss value. */
const transitLossJournal=`SELECT d.company_id,e.journal_id id FROM inventory_transit_events e
 JOIN inventory_transit_events p ON p.id=e.proposal_id AND p.transit_id=e.transit_id AND p.line_id=e.line_id AND p.kind='loss_requested' AND p.quantity_milli=e.quantity_milli
 JOIN inventory_transit_lines l ON l.id=e.line_id AND l.transit_id=e.transit_id
 JOIN inventory_transit d ON d.id=e.transit_id
 JOIN inventory_transfers t ON t.id=d.transfer_id AND t.company_id=d.company_id AND t.source_warehouse_id=d.source_warehouse_id AND t.target_warehouse_id=d.target_warehouse_id AND t.source_branch_id=d.source_branch_id AND t.target_branch_id=d.target_branch_id AND t.delivery_mode='transit' AND t.status='posted'
 JOIN finance_journals j ON j.id=e.journal_id AND j.company_id=d.company_id AND j.chart_id=d.chart_id AND j.currency=d.currency AND j.source_type='inventory_transit_loss' AND j.source_id=p.id AND j.status='posted'
 WHERE e.kind='loss_approved' AND e.quantity_milli>0 AND e.value_minor>0 AND d.dispatch_date IS NOT NULL AND d.status IN ('in_transit','completed')
 AND (SELECT sum(debit_minor) FROM finance_journal_lines WHERE journal_id=j.id)=e.value_minor AND (SELECT sum(credit_minor) FROM finance_journal_lines WHERE journal_id=j.id)=e.value_minor`;
const salesQualityCountJournal=`SELECT source.company_id,c.journal_id id FROM (${qualityDisposalCountRows}) source JOIN inventory_counts c ON c.id=source.id AND c.company_id=source.company_id AND c.status='posted' JOIN finance_journals j ON j.id=c.journal_id AND j.company_id=c.company_id AND j.source_type='stock_count' AND j.source_id=c.id AND j.status='posted' WHERE source.kind='customer-return'`;
const salesInitialPhysicalCountJournal=`SELECT source.company_id,c.journal_id id FROM (${initialPhysicalStockCountSourceRows}) source JOIN inventory_counts c ON c.id=source.id AND c.company_id=source.company_id AND c.status='posted' JOIN finance_journals j ON j.id=c.journal_id AND j.company_id=c.company_id AND j.chart_id=source.source_proof->>'chartId' AND j.currency=source.source_proof->>'currency' AND j.source_type='stock_count' AND j.source_id=c.id AND j.status='posted' AND j.is_opening=0`;
const landedCostJournal=`WITH landed_finance_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) SELECT source.company_id,c.journal_id id FROM (${landedCostPurchaseSourceRows}) source JOIN inventory_landed_costs c ON c.id=source.id AND c.company_id=source.company_id AND c.status='posted' JOIN finance_journals j ON j.id=c.journal_id AND j.company_id=c.company_id AND j.source_type='landed_cost' AND j.source_id=c.id AND j.status='posted' AND j.currency=c.currency CROSS JOIN landed_finance_clock WHERE ${purchasedCapabilityPredicate('source.company_id','supply.advanced-inventory','history','landed_finance_clock.now_ms')} AND (SELECT sum(debit_minor) FROM finance_journal_lines WHERE journal_id=j.id)=c.amount_minor AND (SELECT sum(credit_minor) FROM finance_journal_lines WHERE journal_id=j.id)=c.amount_minor`;
const manufacturingInitialPhysicalCountJournal=`SELECT source.company_id,c.journal_id id FROM (${manufacturingInitialPhysicalStockCountSourceRows}) source JOIN inventory_counts c ON c.id=source.id AND c.company_id=source.company_id AND c.status='posted' JOIN finance_journals j ON j.id=c.journal_id AND j.company_id=c.company_id AND j.chart_id=source.source_proof->>'chartId' AND j.currency=source.source_proof->>'currency' AND j.source_type='stock_count' AND j.source_id=c.id AND j.status='posted' AND j.is_opening=0
 WHERE c.net_value_minor>0 AND c.net_value_minor=(SELECT sum(line.counted_value_minor-line.snapshot_value_minor) FROM inventory_count_lines line WHERE line.count_id=c.id)
 AND (SELECT count(*) FROM finance_journal_lines line WHERE line.journal_id=j.id)=2
 AND EXISTS(SELECT 1 FROM finance_journal_lines line WHERE line.journal_id=j.id AND line.account_id=source.source_proof->>'inventoryAccountId' AND line.debit_minor=c.net_value_minor AND line.credit_minor=0)
 AND EXISTS(SELECT 1 FROM finance_journal_lines line WHERE line.journal_id=j.id AND line.account_id=source.source_proof->>'varianceAccountId' AND line.credit_minor=c.net_value_minor AND line.debit_minor=0)
 AND EXISTS(SELECT 1 FROM (${supplyInventoryTerminalReceipts}) terminal WHERE terminal.kind='count' AND terminal.id=c.id AND terminal.company_id=c.company_id AND terminal.status=c.status AND terminal.version=c.version)`;
const manufacturingInitialPhysicalCountMarkedJournal=`SELECT c.company_id,c.journal_id id FROM inventory_counts c JOIN (${manufacturingInitialPhysicalStockMarkedRows}) marker ON marker.id=c.id WHERE c.journal_id IS NOT NULL`;
const posInitialPhysicalCountJournal=`SELECT source.company_id,c.journal_id id FROM (${posInitialPhysicalStockCountSourceRows}) source JOIN inventory_counts c ON c.id=source.id AND c.company_id=source.company_id AND c.status='posted' JOIN finance_journals j ON j.id=c.journal_id AND j.company_id=c.company_id AND j.chart_id=source.source_proof->>'chartId' AND j.currency=source.source_proof->>'currency' AND j.source_type='stock_count' AND j.source_id=c.id AND j.status='posted' AND j.is_opening=0
 WHERE c.net_value_minor>0 AND c.net_value_minor=(SELECT sum(line.counted_value_minor-line.snapshot_value_minor) FROM inventory_count_lines line WHERE line.count_id=c.id)
 AND (SELECT count(*) FROM finance_journal_lines line WHERE line.journal_id=j.id)=2
 AND EXISTS(SELECT 1 FROM finance_journal_lines line WHERE line.journal_id=j.id AND line.account_id=source.source_proof->>'inventoryAccountId' AND line.debit_minor=c.net_value_minor AND line.credit_minor=0)
 AND EXISTS(SELECT 1 FROM finance_journal_lines line WHERE line.journal_id=j.id AND line.account_id=source.source_proof->>'varianceAccountId' AND line.credit_minor=c.net_value_minor AND line.debit_minor=0)
 AND EXISTS(SELECT 1 FROM (${supplyInventoryTerminalReceipts}) terminal WHERE terminal.kind='count' AND terminal.id=c.id AND terminal.company_id=c.company_id AND terminal.status=c.status AND terminal.version=c.version)`;
const posInitialPhysicalCountMarkedJournal=`SELECT c.company_id,c.journal_id id FROM inventory_counts c JOIN (${posInitialPhysicalStockMarkedRows}) marker ON marker.id=c.id WHERE c.journal_id IS NOT NULL`;
const serviceInitialPhysicalCountJournal=`SELECT source.company_id,c.journal_id id FROM (${serviceInitialPhysicalStockCountSourceRows}) source JOIN inventory_counts c ON c.id=source.id AND c.company_id=source.company_id AND c.status='posted' JOIN finance_journals j ON j.id=c.journal_id AND j.company_id=c.company_id AND j.chart_id=source.source_proof->>'chartId' AND j.currency=source.source_proof->>'currency' AND j.source_type='stock_count' AND j.source_id=c.id AND j.status='posted' AND j.is_opening=0
 WHERE c.net_value_minor>0 AND c.net_value_minor=(SELECT sum(line.counted_value_minor-line.snapshot_value_minor) FROM inventory_count_lines line WHERE line.count_id=c.id)
 AND (SELECT count(*) FROM finance_journal_lines line WHERE line.journal_id=j.id)=2
 AND EXISTS(SELECT 1 FROM finance_journal_lines line WHERE line.journal_id=j.id AND line.account_id=source.source_proof->>'inventoryAccountId' AND line.debit_minor=c.net_value_minor AND line.credit_minor=0)
 AND EXISTS(SELECT 1 FROM finance_journal_lines line WHERE line.journal_id=j.id AND line.account_id=source.source_proof->>'varianceAccountId' AND line.credit_minor=c.net_value_minor AND line.debit_minor=0)
 AND EXISTS(SELECT 1 FROM (${supplyInventoryTerminalReceipts}) terminal WHERE terminal.kind='count' AND terminal.id=c.id AND terminal.company_id=c.company_id AND terminal.status=c.status AND terminal.version=c.version)`;
const serviceInitialPhysicalCountMarkedJournal=`SELECT c.company_id,c.journal_id id FROM inventory_counts c JOIN (${serviceInitialPhysicalStockMarkedRows}) marker ON marker.id=c.id WHERE c.journal_id IS NOT NULL`;
const initialPhysicalCountMarkedJournal=`SELECT c.company_id,c.journal_id id FROM inventory_counts c JOIN (${initialPhysicalStockMarkedRows}) marker ON marker.id=c.id WHERE c.journal_id IS NOT NULL`;
export const inventoryFinanceRecordSources={serviceInitialPhysicalCountJournal,serviceInitialPhysicalCountMarkedJournal,posInitialPhysicalCountJournal,posInitialPhysicalCountMarkedJournal,manufacturingInitialPhysicalCountJournal,manufacturingInitialPhysicalCountMarkedJournal,initialPhysicalCountMarkedJournal,countJournal,transitLossJournal,salesQualityCountJournal,salesInitialPhysicalCountJournal,salesCountJournal:`${salesQualityCountJournal} UNION ${salesInitialPhysicalCountJournal}`,landedCostJournal,journal:`${countJournal} UNION ${transitLossJournal} UNION (${landedCostJournal})`} as const;
/** Relation leases precede the first business snapshot, including an absent
 * Company source. Files/Finance may then inspect stable actual relationships. */
export function inventoryFinanceSourceReadLocks(database:SqlDatabase){return [...qualityPurchaseSourceReadLocks(database),...landedCostSourceReadLocks(database),database.prepare("LOCK TABLE inventory_transfers,inventory_transit,inventory_transit_lines,inventory_transit_events IN SHARE MODE")];}

/** Full private proof for Finance byte readers; original tables only. */
export const manufacturingInitialPhysicalCountFinanceImages=`SELECT source.company_id,source.journal_id id,source.image FROM (${manufacturingInitialPhysicalStockSourceImages}) source WHERE source.journal_id IS NOT NULL`;

export const posInitialPhysicalCountFinanceImages=`SELECT source.company_id,source.journal_id id,source.image FROM (${posInitialPhysicalStockSourceImages}) source WHERE source.journal_id IS NOT NULL`;

export const serviceInitialPhysicalCountFinanceImages=`SELECT source.company_id,source.journal_id id,source.image FROM (${serviceInitialPhysicalStockSourceImages}) source WHERE source.journal_id IS NOT NULL`;
