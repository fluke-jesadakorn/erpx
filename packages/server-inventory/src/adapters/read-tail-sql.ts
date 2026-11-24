import 'server-only';
import {packageClockSql} from '@erpx/server-commercial/public/purchased-capability-sql';
import {platformOwnerProjectionSources as identitySources} from '@erpx/server-identity_access/public/platform-owner-projection-sources';
import {inventoryQualitySnapshotSources} from '@erpx/server-quality/public/inventory-quality-snapshot-sources';

/** Byte-identical SQL text shared by the retained TypeScript read tail.
 * Every consumer issues its own independent statement with its own binds; only
 * the literal text is deduplicated. */
const quality=inventoryQualitySnapshotSources();
export const initialEntryClockAssignmentsSql=`WITH initial_entry_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms) SELECT a.id FROM (${identitySources.assignments}) a CROSS JOIN (SELECT ?::text company_id,?::text branch_id,NULL::text department_id) initial_entry_scope CROSS JOIN initial_entry_clock`;
export const stockHistoryExistsSql=`SELECT EXISTS(SELECT 1 FROM commerce_stock s WHERE s.warehouse_id=? AND s.product_id=?) OR EXISTS(SELECT 1 FROM inventory_ledger m WHERE m.warehouse_id=? AND m.product_id=?) OR EXISTS(SELECT 1 FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id WHERE b.warehouse_id=? AND l.product_id=?) OR EXISTS(SELECT 1 FROM inventory_order_reservations r WHERE r.warehouse_id=? AND r.product_id=?) OR EXISTS(SELECT 1 ${quality.inventory_counts_inventory_quality_holds_from} WHERE warehouse_id=? AND product_id=?) OR EXISTS(SELECT 1 FROM inventory_count_lines l JOIN inventory_counts c ON c.id=l.count_id WHERE c.warehouse_id=? AND l.product_id=? AND (?::text IS NULL OR c.id<>?) AND (c.status<>'cancelled' OR c.posted_at IS NOT NULL OR c.journal_id IS NOT NULL)) history`;
export const noLaterLedgerMovementSql="NOT EXISTS(SELECT 1 FROM inventory_ledger WHERE warehouse_id=? AND product_id=? AND movement_date>?)";
export const noLotBalanceSql="NOT EXISTS(SELECT 1 FROM inventory_lot_balances WHERE lot_id=? AND warehouse_id=?)";
export const serviceInitialPhysicalNativeRowSql='SELECT 1 FROM inventory_service_initial_physical_sources WHERE count_id=? AND company_id=?';
export const jsonbImageSql='SELECT jsonb_build_object(';
/** r11: the reservations rollup and the single-count-line marker are each
 * byte-identical across their consumers; only the literal text is shared. */
export const reservedWarehouseProductQtySql='SELECT warehouse_id,product_id,sum(quantity_milli::numeric) quantity_milli';
export const singleCountLineMarkerSql='(SELECT count(*) FROM inventory_count_lines actual WHERE actual.count_id=c.id)=1';
/** r12: byte-identical schema-readiness checks, each repeated within one command.
 * Only the literal text is shared; every caller prepares its own statement. */
export const posInitialPhysicalSchemaReadySql="SELECT to_regclass('inventory_pos_initial_physical_sources') IS NOT NULL ready";
export const mfgInitialPhysicalSchemaReadySql="SELECT to_regclass('inventory_mfg_initial_physical_sources') IS NOT NULL ready";
/** r13: byte-identical read-tail fragments. Only the literal text is shared;
 * every consumer issues its own statement with its own rows, binds and scope. */
export const countMarkedExistsSql=(rows:string)=>`SELECT 1 FROM (${rows}) marker WHERE id=?`;
export const countSourceProofSql=(rows:string)=>`SELECT source_proof::text proof FROM (${rows}) source WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?)`;
export const countCreateMarkedRowsSql=(marker:string)=>`SELECT record_id id,authority_json::jsonb->>'companyId' company_id FROM command_receipts WHERE operation='inventory.count.create' AND jsonb_exists(result_json::jsonb,'${marker}')`;
export const countMarkedJournalSql=(rows:string)=>`SELECT c.company_id,c.journal_id id FROM inventory_counts c JOIN (${rows}) marker ON marker.id=c.id WHERE c.journal_id IS NOT NULL`;
export const allInitialPhysicalStockMarkedRowsSql=(...rows:string[])=>`SELECT 1 FROM (${rows.join(' UNION ALL ')})`;
/** r14: runner-up byte-identical fragments. Only the literal text is shared;
 * every consumer keeps its own rows, alias, binds and independent statement. */
export const countInitialMarkedExistsSql=(rows:string,id:string,alias='initial_marker')=>`EXISTS(SELECT 1 FROM (${rows}) ${alias} WHERE ${alias}.id=${id})`;
export const initialPhysicalCountJournalSql=(rows:string,tail:string)=>`SELECT source.company_id,c.journal_id id FROM (${rows}) source JOIN inventory_counts c ON c.id=source.id AND c.company_id=source.company_id AND c.status='posted' JOIN finance_journals j ON j.id=c.journal_id AND j.company_id=c.company_id AND j.chart_id=source.source_proof->>'chartId' AND j.currency=source.source_proof->>'currency' AND j.source_type='stock_count' AND j.source_id=c.id AND j.status='posted' AND j.is_opening=0${tail}`;
export const initialPhysicalCountFinanceImagesSql=(images:string)=>`SELECT source.company_id,source.journal_id id,source.image FROM (${images}) source WHERE source.journal_id IS NOT NULL`;
/** r15: parameterized child-document selectors. The table, row alias, join and
 * scope vary per consumer; only the SQL text is shared. Every caller still
 * prepares its own statement with its own rows, binds and scope. r11-r14
 * helpers above are left untouched. */
export const childJsonAggSql=(table:string,alias:string,where:string)=>`(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id) FROM ${table} ${alias} WHERE ${where})`;
export const childJoinJsonAggSql=(table:string,alias:string,join:string,where:string)=>`(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id) FROM ${table} ${alias} JOIN ${join} WHERE ${where})`;
/** r16: remaining parameterized EXISTS guard families. The terminal-receipt
 * match and the transit loss-decision guard are each byte-identical across
 * their consumers; only the rows, kind, alias and row expression vary. Every
 * caller keeps its own rows, binds and independent statement. r11-r15 helpers
 * above are left untouched. */
export const terminalReceiptMatchSql = (rows: string, kind: string, id: string, company: string, status: string, version: string) =>
  `EXISTS(SELECT 1 FROM (${rows}) terminal WHERE terminal.kind='${kind}' AND terminal.id=${id} AND terminal.company_id=${company} AND terminal.status=${status} AND terminal.version=${version})`;
export const transitLossDecisionPendingSql = (alias: string, id: string) =>
  `NOT EXISTS(SELECT 1 FROM inventory_transit_events ${alias} WHERE ${alias}.proposal_id=${id})`;
/** r17: remaining parameterized aggregate/EXISTS families. The active-reservation
 * SUM, the quality-hold quality_held guard and the initial-count image probe are
 * each byte-identical across their consumers; only the scope predicate, the hold
 * source and the image source vary. Every caller keeps its own binds and its own
 * independent statement. r11-r16 helpers above are left untouched. */
export const reservedStockAggSql = (predicate: string) =>
  `COALESCE((SELECT SUM(r.quantity_milli) FROM inventory_active_reservations r WHERE ${predicate}),0)`;
export const qualityHeldExistsSql = (holds: string, predicate: string) =>
  `CASE WHEN EXISTS(SELECT 1 FROM (${holds}) h WHERE ${predicate}) THEN 1 ELSE 0 END quality_held`;
export const initialPhysicalCountImageSql = (images: string) =>
  `SELECT image FROM (${images}) source WHERE id=? AND company_id=? AND EXISTS(SELECT 1 FROM inventory_counts c WHERE c.id=source.id AND (?::text IS NULL OR c.branch_id=?))`;
/** r18: remaining parameterized EXISTS/aggregate families. The active quality-hold
 * EXISTS guard and the pending-loss SUM are each byte-identical across their
 * consumers; only the source relation, metric and scope vary. Every caller keeps
 * its own source call, binds and independent statement. r11-r17 helpers above are
 * left untouched. */
export const activeQualityHoldExistsSql = (source: string, predicate: string) =>
  `EXISTS(SELECT 1 ${source} WHERE ${predicate})`;
export const pendingLossSumSql = (metric: string, from: string, where: string) =>
  `COALESCE((SELECT SUM(${metric}) ${from} WHERE ${where}),0)`;
/** r19: parameterized settlement-move SUM. The delivery and return stock
 * reconciliations repeat the same COALESCE(sum(...)) probe; only the metric,
 * move source and scope predicate vary. Every caller keeps its own source,
 * binds and independent statement. r11-r18 helpers above are left untouched. */
export const stockMoveSumSql = (metric: string, moves: string, where: string) =>
  `COALESCE((SELECT sum(${metric}) FROM (${moves}) move WHERE ${where}),0)`;
/** r20: the four initial-physical source markers are byte-identical across the
 * count list and paged-list reads; only the four source-row fragments vary.
 * Every caller prepares its own statement with its own binds. r11-r19 helpers
 * above are left untouched. */
export const countInitialPhysicalSourceMarkersSql = (initial: string, mfg: string, pos: string, service: string) =>
  `EXISTS(SELECT 1 FROM (${initial}) actual_initial WHERE actual_initial.id=c.id AND actual_initial.company_id=c.company_id) initial_physical_source,` +
  `EXISTS(SELECT 1 FROM (${mfg}) actual_mfg_initial WHERE actual_mfg_initial.id=c.id AND actual_mfg_initial.company_id=c.company_id) manufacturing_initial_physical_source,` +
  `EXISTS(SELECT 1 FROM (${pos}) actual_pos_initial WHERE actual_pos_initial.id=c.id AND actual_pos_initial.company_id=c.company_id) pos_initial_physical_source,` +
  `EXISTS(SELECT 1 FROM (${service}) actual_service_initial WHERE actual_service_initial.id=c.id AND actual_service_initial.company_id=c.company_id) service_initial_physical_source`;
/** r20: the posted initial-physical stock source probe is byte-identical across
 * the sales, manufacturing and POS package-obligation reads; only the source
 * rows and the posted alias vary. Every caller keeps its own statement, scope
 * and binds. */
export const postedInitialPhysicalStockSourceSql = (source: string, posted: string) =>
  `EXISTS(SELECT 1 FROM (${source}) source JOIN inventory_counts ${posted} ON ${posted}.id=source.id AND ${posted}.company_id=source.company_id AND ${posted}.status='posted' WHERE source.company_id=scope.company_id AND source.warehouse_id=stock.warehouse_id AND source.product_id=stock.product_id)`;
