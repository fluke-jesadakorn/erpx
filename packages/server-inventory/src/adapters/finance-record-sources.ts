import {serviceInitialPhysicalStockCountSourceRows,serviceInitialPhysicalStockMarkedRows,serviceInitialPhysicalStockSourceImages} from '@erpx/server-inventory/adapters/service-initial-physical-stock-count-source-sql';
import {posInitialPhysicalStockCountSourceRows,posInitialPhysicalStockMarkedRows,posInitialPhysicalStockSourceImages} from '@erpx/server-inventory/adapters/pos-initial-physical-stock-count-source-sql';
import {supplyInventoryTerminalReceipts} from '@erpx/server-inventory/adapters/supply-terminal-receipts';
import {manufacturingInitialPhysicalStockCountSourceRows,manufacturingInitialPhysicalStockMarkedRows,manufacturingInitialPhysicalStockSourceImages} from '@erpx/server-inventory/adapters/manufacturing-initial-physical-stock-count-source-sql';
import {initialPhysicalStockCountSourceRows,initialPhysicalStockMarkedRows} from '@erpx/server-inventory/adapters/initial-physical-stock-count-source-sql';
import {purchasedCapabilityPredicate} from "@erpx/server-commercial/public/purchased-capability-sql";
import "server-only";
import {landedCostPurchaseSourceRows,landedCostSourceReadLocks} from "@erpx/server-inventory/adapters/landed-cost-purchases";
import {qualityDisposalCountRows} from "@erpx/server-inventory/adapters/quality-count-purchases";
import {qualityPurchaseSourceReadLocks} from "@erpx/server-quality/public/purchase-sources";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {clockCteSql,countMarkedJournalSql,allInitialPhysicalStockMarkedRowsSql,initialPhysicalCountJournalSql,initialPhysicalCountFinanceImagesSql,journalAccountLineExistsSql,terminalReceiptMatchSql,journalLineSumSql} from './read-tail-sql';

/** r11: the posted-count journal tail is byte-identical for the manufacturing,
 * pos and service initial-physical-count variants. Only the literal text is
 * shared; each statement keeps its own source rows, binds and execution. */
const initialPhysicalCountJournalTail=`
 WHERE c.net_value_minor>0 AND c.net_value_minor=(SELECT sum(line.counted_value_minor-line.snapshot_value_minor) FROM inventory_count_lines line WHERE line.count_id=c.id)
 AND (SELECT count(*) FROM finance_journal_lines line WHERE line.journal_id=j.id)=2
 AND ${journalAccountLineExistsSql('inventoryAccountId','debit_minor','credit_minor')}
 AND ${journalAccountLineExistsSql('varianceAccountId','credit_minor','debit_minor')}
 AND ${terminalReceiptMatchSql(supplyInventoryTerminalReceipts,'count','c.id','c.company_id','c.status','c.version')}`;

/** Exact posted physical-count ownership. Count commands own independent
 * stock/GL posting; keeping this alias preserves its established history port. */
const countJournal=`SELECT c.company_id,c.journal_id id FROM inventory_counts c JOIN finance_journals j ON j.id=c.journal_id AND j.company_id=c.company_id AND j.source_type='stock_count' AND j.source_id=c.id AND j.status='posted' WHERE c.status='posted' AND NOT EXISTS(${allInitialPhysicalStockMarkedRowsSql(initialPhysicalStockMarkedRows,manufacturingInitialPhysicalStockMarkedRows,posInitialPhysicalStockMarkedRows,serviceInitialPhysicalStockMarkedRows)} marker WHERE marker.id=c.id)`;
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
 AND ${journalLineSumSql('debit_minor')}=e.value_minor AND ${journalLineSumSql('credit_minor')}=e.value_minor`;
const salesQualityCountJournal=`SELECT source.company_id,c.journal_id id FROM (${qualityDisposalCountRows}) source JOIN inventory_counts c ON c.id=source.id AND c.company_id=source.company_id AND c.status='posted' JOIN finance_journals j ON j.id=c.journal_id AND j.company_id=c.company_id AND j.source_type='stock_count' AND j.source_id=c.id AND j.status='posted' WHERE source.kind='customer-return'`;
const salesInitialPhysicalCountJournal=initialPhysicalCountJournalSql(initialPhysicalStockCountSourceRows,'');
const landedCostJournal=`WITH ${clockCteSql('landed_finance_clock')} SELECT source.company_id,c.journal_id id FROM (${landedCostPurchaseSourceRows}) source JOIN inventory_landed_costs c ON c.id=source.id AND c.company_id=source.company_id AND c.status='posted' JOIN finance_journals j ON j.id=c.journal_id AND j.company_id=c.company_id AND j.source_type='landed_cost' AND j.source_id=c.id AND j.status='posted' AND j.currency=c.currency CROSS JOIN landed_finance_clock WHERE ${purchasedCapabilityPredicate('source.company_id','supply.advanced-inventory','history','landed_finance_clock.now_ms')} AND ${journalLineSumSql('debit_minor')}=c.amount_minor AND ${journalLineSumSql('credit_minor')}=c.amount_minor`;
const manufacturingInitialPhysicalCountJournal=initialPhysicalCountJournalSql(manufacturingInitialPhysicalStockCountSourceRows,initialPhysicalCountJournalTail);
const manufacturingInitialPhysicalCountMarkedJournal=countMarkedJournalSql(manufacturingInitialPhysicalStockMarkedRows);
const posInitialPhysicalCountJournal=initialPhysicalCountJournalSql(posInitialPhysicalStockCountSourceRows,initialPhysicalCountJournalTail);
const posInitialPhysicalCountMarkedJournal=countMarkedJournalSql(posInitialPhysicalStockMarkedRows);
const serviceInitialPhysicalCountJournal=initialPhysicalCountJournalSql(serviceInitialPhysicalStockCountSourceRows,initialPhysicalCountJournalTail);
const serviceInitialPhysicalCountMarkedJournal=countMarkedJournalSql(serviceInitialPhysicalStockMarkedRows);
const initialPhysicalCountMarkedJournal=countMarkedJournalSql(initialPhysicalStockMarkedRows);
export const inventoryFinanceRecordSources={serviceInitialPhysicalCountJournal,serviceInitialPhysicalCountMarkedJournal,posInitialPhysicalCountJournal,posInitialPhysicalCountMarkedJournal,manufacturingInitialPhysicalCountJournal,manufacturingInitialPhysicalCountMarkedJournal,initialPhysicalCountMarkedJournal,countJournal,transitLossJournal,salesQualityCountJournal,salesInitialPhysicalCountJournal,salesCountJournal:`${salesQualityCountJournal} UNION ${salesInitialPhysicalCountJournal}`,landedCostJournal,journal:`${countJournal} UNION ${transitLossJournal} UNION (${landedCostJournal})`} as const;
/** Relation leases precede the first business snapshot, including an absent
 * Company source. Files/Finance may then inspect stable actual relationships. */
export function inventoryFinanceSourceReadLocks(database:SqlDatabase){return [...qualityPurchaseSourceReadLocks(database),...landedCostSourceReadLocks(database),database.prepare("LOCK TABLE inventory_transfers,inventory_transit,inventory_transit_lines,inventory_transit_events IN SHARE MODE")];}

/** Full private proof for Finance byte readers; original tables only. */
export const manufacturingInitialPhysicalCountFinanceImages=initialPhysicalCountFinanceImagesSql(manufacturingInitialPhysicalStockSourceImages);

export const posInitialPhysicalCountFinanceImages=initialPhysicalCountFinanceImagesSql(posInitialPhysicalStockSourceImages);

export const serviceInitialPhysicalCountFinanceImages=initialPhysicalCountFinanceImagesSql(serviceInitialPhysicalStockSourceImages);
