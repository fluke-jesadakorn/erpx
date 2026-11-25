import 'server-only';
import {assignmentReadSql} from '@erpx/server-identity_access/public/record-source-authority';
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
/** r35: the lot-balance existence guards share one byte-identical prefix and
 * differ only in a trailing predicate (a bare version check, a quantity
 * watermark, or the full optimistic-concurrency tuple). Only the literal text
 * is shared; every caller still prepares its own statement with its own binds. */
export const lotBalanceGuardSql = (predicate:string) => `EXISTS(SELECT 1 FROM inventory_lot_balances WHERE lot_id=? AND warehouse_id=?${predicate})`;
export const serviceInitialPhysicalNativeRowSql='SELECT 1 FROM inventory_service_initial_physical_sources WHERE count_id=? AND company_id=?';
export const jsonbImageSql='SELECT jsonb_build_object(';
/** r11: the reservations rollup and the single-count-line marker are each
 * byte-identical across their consumers; only the literal text is shared. */
export const reservedWarehouseProductQtySql='SELECT warehouse_id,product_id,sum(quantity_milli::numeric) quantity_milli';
export const singleCountLineMarkerSql='(SELECT count(*) FROM inventory_count_lines actual WHERE actual.count_id=c.id)=1';
/** r12: byte-identical schema-readiness checks, each repeated within one command.
 * Only the literal text is shared; every caller prepares its own statement. */
/** r46: the three reviewed to_regclass schema probes share one whitelisted
 * builder. Only the table varies; every caller keeps its own prepare/bind. */
const schemaReadyTables=new Set(['inventory_initial_physical_stock_sources','inventory_pos_initial_physical_sources','inventory_mfg_initial_physical_sources']);
export const schemaReadySql=(table:string)=>{if(!schemaReadyTables.has(table))throw new Error(`INVENTORY_SCHEMA_READY_TABLE_NOT_ALLOWED:${table}`);return `SELECT to_regclass('${table}') IS NOT NULL ready`;};
export const posInitialPhysicalSchemaReadySql=schemaReadySql('inventory_pos_initial_physical_sources');
export const mfgInitialPhysicalSchemaReadySql=schemaReadySql('inventory_mfg_initial_physical_sources');
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
/** r21: parameterized count-source EXISTS guards. The source rows, row alias,
 *  scope and capability/module predicates vary per consumer; only the SQL text
 *  is shared. Every caller keeps its own binds and independent statement.
 *  r11-r20 helpers above are left untouched. */
export const sourceRowsExistsSql = (rows: string, alias: string, id: string, company: string, predicates: string) =>
  `EXISTS(SELECT 1 FROM (${rows}) ${alias} WHERE ${alias}.id=${id} AND ${alias}.company_id=${company} AND ${predicates})`;
/** r21: parameterized attachment source-proof EXISTS guard. The source rows,
 *  row alias and feature-module predicates vary per consumer; only the SQL text
 *  is shared. Every caller keeps its own binds and independent statement. */
export const attachmentSourceProofExistsSql = (rows: string, alias: string, modules: string) =>
  `EXISTS(SELECT 1 FROM (${rows}) ${alias} WHERE ${alias}.id=? AND ${alias}.company_id=? AND ${alias}.source_proof::text=? AND ${modules})`;
/** r22: parameterized source-proof EXISTS guards without capability or module
 * predicates. The source rows, row alias and count scope vary per consumer;
 * only the SQL text is shared. Every caller keeps its own binds and statement.
 * r11-r21 helpers above are left untouched. */
export const sourceExistsSql = (rows: string, alias: string, id: string, company: string) =>
  `EXISTS(SELECT 1 FROM (${rows}) ${alias} WHERE ${alias}.id=${id} AND ${alias}.company_id=${company})`;
/** r22: parameterized journal-line aggregate. The debit/credit column varies
 * per term; only the literal text is shared. Every caller keeps its own binds
 * and independent statement. r11-r21 helpers above are left untouched. */
export const journalLineSumSql = (column: string) =>
  `(SELECT sum(${column}) FROM finance_journal_lines WHERE journal_id=j.id)`;
/** r23: parameterized child-line ownership EXISTS guard. A child composite row
 * (count/landed/transit/transfer lot, allocation or event) is admitted only
 * while its owning line still belongs to the parent document. The table, row
 * alias, child FK and parent FK vary per consumer; only the SQL text is shared.
 * Every caller keeps its own statement, binds and scope. r11-r22 helpers above
 * are left untouched. */
export const childLineOwnerExistsSql = (table: string, alias: string, childFk: string, parentFk: string) =>
  `EXISTS(SELECT 1 FROM ${table} ${alias} WHERE ${alias}.id=child.${childFk} AND ${alias}.${parentFk}=d.id)`;
/** r24: parameterized native-record identity EXISTS guard. A records-surface
 *  filter admits a row only while its backing native Inventory document still
 *  exists. The table, row alias and id expression vary per consumer; only the
 *  SQL text is shared. Every caller keeps its own binds and independent
 *  statement. r11-r23 helpers above are left untouched. */
export const nativeRecordExistsSql = (table: string, alias: string, id: string) =>
  `EXISTS(SELECT 1 FROM ${table} ${alias} WHERE ${alias}.id=${id})`;
/** r24: parameterized branch-scope tuple. A branch-restricted read keeps both
 *  the null-branch and matching-branch cases in a single bind pair. Only the
 *  row alias varies; every caller supplies its own two branch binds. */
export const branchScopeTupleSql = (alias: string) =>
  `(?::text IS NULL OR ${alias}.branch_id=?)`;
/** r25: parameterized two-predicate table EXISTS guard. A source/child row is
 *  admitted only while exactly two scope columns match their caller values. The
 *  relation, row alias, both columns and both right-hand expressions vary per
 *  consumer; only the SQL text is shared. Every caller keeps its own statement,
 *  binds and scope. r11-r24 helpers above are left untouched. */
export const scopeExistsSql = (table: string, alias: string, firstColumn: string, firstValue: string, secondColumn: string, secondValue: string) =>
  `EXISTS(SELECT 1 FROM ${table} ${alias} WHERE ${alias}.${firstColumn}=${firstValue} AND ${alias}.${secondColumn}=${secondValue})`;
/** r25: parameterized posted-journal account-line EXISTS guard. A count's
 *  inventory/variance journal leg must carry the exact net value on its debit or
 *  credit column with the opposite leg zero. The source-proof key and the two
 *  amount columns vary per term; only the SQL text is shared. Every caller keeps
 *  its own statement, binds and journal scope. r11-r24 helpers above are left
 *  untouched. */
export const journalAccountLineExistsSql = (accountKey: string, amountColumn: string, zeroColumn: string) =>
  `EXISTS(SELECT 1 FROM finance_journal_lines line WHERE line.journal_id=j.id AND line.account_id=source.source_proof->>'${accountKey}' AND line.${amountColumn}=c.net_value_minor AND line.${zeroColumn}=0)`;
/** r26: parameterized source-row scope projection. A purchase/quality read wraps
 *  its proven source rows in a uniform scope relation that exposes a NULL
 *  department_id for assignment authority checks. The source rows and row alias
 *  vary per consumer; only the SQL text is shared. Every caller keeps its own
 *  statement, binds and scope. r11-r25 helpers above are left untouched. */
export const sourceScopeRowsSql = (rows: string, alias: string) =>
  `SELECT ${alias}.*,NULL::text department_id FROM (${rows}) ${alias}`;
/** r27: parameterized package-obligation document rows. The transfer, transit
 *  and valuation pending-document probes are byte-identical across the POS and
 *  PMS package reads except for the trailing pending projection. Only the SQL
 *  text is shared; every caller keeps its own statement, binds and scope.
 *  r11-r26 helpers above are left untouched. */
export const packageObligationDocumentRowsSql = (pending: string) =>
  `    UNION ALL SELECT 'transfer',t.id,t.version,t.status,t.total_value_minor${pending} FROM inventory_transfers t,scope WHERE t.company_id=scope.company_id AND t.status NOT IN ('posted','cancelled','rejected')\n` +
  `    UNION ALL SELECT 'transit',t.id,t.version,t.status,0${pending} FROM inventory_transit t,scope WHERE t.company_id=scope.company_id AND t.status NOT IN ('completed','cancelled')\n` +
  `    UNION ALL SELECT 'valuation',l.id,l.version,l.status,l.amount_minor${pending} FROM inventory_landed_costs l,scope WHERE l.company_id=scope.company_id AND l.status NOT IN ('posted','cancelled','rejected')`;
/** r28: parameterized executive-evidence position page wrapper. The available-stock,
 *  inventory-shortage and stocked-location searches wrap their source relation and
 *  caller conditions before a uniform warehouse/product order and limit. Only the
 *  SQL text is shared; every caller keeps its own source rows, binds and conditions.
 *  r11-r27 helpers above are left untouched. */
export const evidencePositionRowsSql = (rows: string, alias: string, conditions: string) =>
  `SELECT * FROM (${rows}) ${alias} WHERE ${conditions} ORDER BY warehouse_id,product_id LIMIT ?`;
/** r29: parameterized native-record id-existence probe. A workspace record read
 *  checks its backing Inventory table by id before purchased-access enforcement.
 *  Only the table varies; every caller keeps its own bind and independent
 *  statement. r11-r28 helpers above are left untouched. */
export const recordIdExistsSql = (table: string) =>
  `SELECT 1 FROM ${table} WHERE id=?`;
/** r30: parameterized child-line row selector. The transfer lot-allocation and
 *  transit-lot reads share the same projection, single line-FK predicate and id
 *  order; only the backing table and its FK column vary. Every caller keeps its
 *  own bind and independent statement. r11-r29 helpers above are left untouched. */
export const childLineRowsSql = (table: string, lineFk: string) =>
  `SELECT * FROM ${table} WHERE ${lineFk}=? ORDER BY id`;
/** r31: parameterized current-assignment scope probe. The landed-cost, count and
 *  transfer purchased-read gates share one assignment/authority envelope plus a
 *  NULL department scope; only the scope alias and permission vary. Every caller
 *  keeps its own binds and independent statement. r11-r30 helpers above are left
 *  untouched. */
export const scopeAllowedSql = (scope: string, permission: string) =>
  `SELECT ${assignmentReadSql(scope, permission)} allowed FROM (SELECT ?::text company_id,?::text branch_id,NULL::text department_id) ${scope}`;
/** r32: parameterized quality-disposal count source. The sales and ecommerce
 *  settlement reads share one byte-identical projection/join; only the quality
 *  hold source rows and the trailing source-type predicate vary. Every caller
 *  keeps its own source rows, binds and independent statement. r11-r31 helpers
 *  above are left untouched. */
export const disposalCountsSql = (holds: string, tail = '') =>
  `SELECT count.*,hold.source_type quality_source_type,hold.source_id quality_source_id FROM inventory_counts count JOIN (${holds}) hold ON hold.id=count.quality_hold_id${tail}`;
/** r33: parameterized single-field row probe. The transit lot-balance version
 *  read and the transfer flow-status probe share one byte-identical projection,
 *  two-key equality predicate and bind shape; only the field, table and scope
 *  keys vary. Every caller keeps its own binds and independent statement. The
 *  canonical inventoryCountStatusSql count-source literal is left untouched.
 *  r11-r32 helpers above are left untouched. */
export const scopedFieldSql = (field: string, table: string, key: string, scopeKey: string) =>
  `SELECT ${field} FROM ${table} WHERE ${key}=? AND ${scopeKey}=?`;
/** r34: parameterized clock-scoped purchase existence envelope. The transfer
 *  and landed-cost record-purchase guards share one byte-identical clock CTE,
 *  two-key scope relation, branch/assignment authority and capability tail;
 *  only the source rows, row alias, permission and capability predicate vary.
 *  Every caller keeps its own binds and independent statement. Locks, count
 *  sources, count adapters and the native registry stay untouched, and the
 *  count-record-purchases count source is deliberately not folded in. r11-r33
 *  helpers above are left unchanged. */
export const clockScopedPurchaseExistsSql = (alias: string, rows: string, id: string, company: string, permission: string, capability: string) =>
  `EXISTS(WITH ${alias}_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) SELECT 1 FROM (${rows}) ${alias} CROSS JOIN ${alias}_clock WHERE ${alias}.id=${id} AND ${alias}.company_id=${company} AND ${branchScopeTupleSql(alias)} AND ${assignmentReadSql(alias, permission)} AND ${capability})`;
/** r36: parameterized executive position source CTE and projection anchor. The
 *  available-stock and stocked-location reads share one byte-identical reservation
 *  CTE and projection through the warehouse scope; only the warehouse projection
 *  rows and the trailing status/scope predicate vary. Every caller keeps its own
 *  binds and independent statement. r11-r35 helpers above are left untouched. */
export const stockPositionSourcesSql = (warehouses: string, tail: string) =>
  `WITH reservations AS (
    ${reservedWarehouseProductQtySql}
    FROM inventory_active_reservations GROUP BY warehouse_id,product_id
  )
  SELECT w.id warehouse_id,p.id product_id,p.sku,p.name product_name,p.unit,w.name warehouse_name,
    s.quantity_milli::text on_hand_milli,COALESCE(r.quantity_milli,0)::text reserved_milli,
    inventory_quarantined_quantity(w.id,p.id)::text quarantined_milli,
    GREATEST(s.quantity_milli::numeric-GREATEST(COALESCE(r.quantity_milli,0),inventory_quarantined_quantity(w.id,p.id)::numeric),0)::text available_milli,
    s.version stock_version
  FROM commerce_stock s JOIN (${warehouses}) w ON w.id=s.warehouse_id AND w.company_id=?${tail}`;
/** r36: parameterized inventory-movement evidence row projection. The search
 *  and point reads share one byte-identical projection/join anchor; only the
 *  warehouse/product projection rows and the trailing predicate vary. Every
 *  caller keeps its own binds and independent statement. r11-r35 helpers above
 *  are left untouched. */
export const inventoryMovementEvidenceRowsSql = (warehouses: string, products: string, tail: string) =>
  `SELECT l.id,l.source_type,l.source_id,l.source_number,l.movement_date,l.quantity_milli::text quantity_milli,p.unit,p.sku,p.name product_name,w.id warehouse_id,w.name warehouse_name
    FROM inventory_ledger l JOIN (${warehouses}) w ON w.id=l.warehouse_id JOIN (${products}) p ON p.id=l.product_id AND p.company_id=w.company_id${tail}`;
/** r36: parameterized executive position point probe. The available-stock and
 *  stocked-location point reads share one byte-identical wrapper; only the
 *  source positions and the numeric availability predicate vary. Every caller
 *  keeps its own binds and independent statement. r11-r35 helpers above are
 *  left untouched. */
export const executivePositionPointSql = (positions: string, condition: string) =>
  `SELECT * FROM (${positions}) stock WHERE ${condition} AND warehouse_id=? AND product_id=?`;
/** r36: parameterized maintenance-settlement stock source columns. The product
 *  and warehouse source probes share one byte-identical two-column projection;
 *  only the column and table vary. Every caller keeps its own bind shape. The
 *  lot source, locks, count sources/adapters and native registry stay untouched.
 *  r11-r35 helpers above are left unchanged. */
export const stockSourceColumnsSql = (column: string, table: string) =>
  `SELECT id,company_id,${column} FROM ${table}`;
/** r37: parameterized attachment-owner version guard. The operational and
 *  service attachment facts share one byte-identical EXISTS guard on the backing
 *  Inventory document's id, company and version; only the relation and an
 *  optional extra predicate vary. Every caller keeps its own binds and
 *  independent assertion statement. The UPDATE version-step statements, locks,
 *  count sources/adapters and native registry stay untouched. r11-r36 helpers
 *  above are left unchanged. */
export const attachmentOwnerVersionGuardSql = (table: string, extra = '') =>
  `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=?${extra ? ` AND ${extra}` : ''})`;
/** r38: parameterized attachment child-lot id ARRAY projection. The landed-cost,
 *  transit and transfer attachment reads share one byte-identical projection of
 *  child lot ids through the owning line; only the lot relation, line relation,
 *  child FK and parent FK vary. Every caller keeps its own bind shape and
 *  independent statement. Locks, count sources/adapters, the clock CTE and the
 *  native read registry stay untouched. r11-r37 helpers above are left unchanged. */
export const attachmentLotIdsSql = (lots: string, lines: string, lineFk: string, parentFk: string) =>
  `ARRAY(SELECT lot_id FROM ${lots} cl JOIN ${lines} l ON l.id=cl.${lineFk} WHERE l.${parentFk}=d.id) lot_ids`;
/** r39: parameterized reserved-stock availability SUM. The production joined
 *  projection's reserved-stock and reserved-availability facts share one
 *  byte-identical SQL head; only the reservation alias and WHERE tail vary.
 *  Every consumer keeps its own binds and independent statement. Locks, count
 *  sources/adapters, the clock CTE and native transport stay untouched.
 *  r11-r38 helpers above are left unchanged. */
export const reservedAvailabilitySumSql = (tail: string) =>
  `COALESCE((SELECT sum(quantity_milli) FROM inventory_active_reservations ${tail}),0)`;
/** r40: parameterized decision-actor projection. The stock-count and warehouse-
 *  transfer decision reads share one byte-identical SQL head; only the decision
 *  table, its parent FK and the actor projection vary. Every consumer keeps its
 *  own bind and independent statement. Locks, count sources/adapters, the clock
 *  CTE and native transport stay untouched. r11-r39 helpers above are left
 *  unchanged. */
export const decisionActorRowsSql = (table: string, fk: string, actor: string) =>
  `SELECT d.*,COALESCE(p.legal_name,d.actor_id) actor_name FROM ${table} d ${actor} WHERE d.${fk}=? ORDER BY d.created_at DESC`;
/** r41: parameterized MATERIALIZED clock-CTE envelope. The five retained
 *  read-tail guards share one byte-identical clock CTE; only the alias varies.
 *  Every caller keeps its own statement, binds and capability/scope tail. Locks,
 *  count sources/adapters, native transport and r11-r40 helpers stay untouched. */
export const clockCteSql = (alias: string) =>
  `${alias} AS MATERIALIZED (SELECT ${packageClockSql} now_ms)`;
/** r42: the POS and PMS package-obligation count-projection heads are
 *  byte-identical; only the trailing projection and scope tail vary. Every
 *  caller keeps its own statement, binds and independent tail. Locks, count
 *  sources/adapters, native transport and r11-r41 helpers stay untouched. */
export const packageObligationCountHeadSql = `WITH scope AS (SELECT ?::text company_id)\n    SELECT 'count' kind,c.id,c.version,c.status,c.net_value_minor amount`;
/** r42: the manufacturing and POS package-obligation explicit count-scope
 *  rows share one byte-identical guard and marker projection; only the marked
 *  row source varies. Every caller supplies its own rows and binds. Locks,
 *  count sources/adapters, native transport and r11-r41 helpers stay untouched. */
export const countScopeRowsSql = (rows: string) =>
  ` c.company_id=scope.company_id AND c.status NOT IN ('posted','cancelled','rejected') AND NOT EXISTS(SELECT 1 FROM (${rows}) marker WHERE marker.id=c.id)`;
/** r43: the four initial-physical-stock evidence readers and the quality
 *  disposal evidence reader all project the same c-alias four-column head.
 *  Each caller supplies the remaining projection/joins verbatim, so every
 *  emitted statement stays byte-identical with its own binds and tail. Locks,
 *  native transport, count adapters and r11-r42 helpers stay untouched. */
export const countEvidenceRowHeadSql = (tail: string) =>
  `SELECT c.id,c.company_id,c.branch_id,c.warehouse_id,${tail}`;

/** r44: the initial-physical-count SourceImages projections share one
 *  byte-identical c-alias select head; only the image/tail varies. Each caller
 *  keeps its own statement, binds and source rows. Locks, native transport,
 *  count adapters and r11-r43 helpers stay untouched. */
export const sourceImagesHeadSql = 'SELECT c.id,c.company_id,c.journal_id,';
/** r44: the lot-balance joined read projections share one byte-identical
 *  balance-column head; only the guard, join and scope tail vary. */
export const lotBalanceRowsHeadSql = 'SELECT l.*,b.warehouse_id,b.quantity_milli,b.value_minor,b.version balance_version,';
/** r44: the count-file authority scope relation is byte-identical across the
 *  attachment and record purchase reads; only the relation alias varies. */
export const countAuthorityScopeSql = (alias: string) =>
  `(SELECT id,company_id,branch_id,NULL::text department_id FROM inventory_counts) ${alias}`;

/** r45: the completed-transit receipt and loss-approval guards project the
 *  same byte-identical transit-event quantity aggregate; only the event kind
 *  varies. Each caller keeps its own statement and binds. Locks, count
 *  sources/adapters, native transport and r11-r44 helpers stay untouched. */
export const transitEventQuantitySumSql = (kind: string) =>
  `SELECT sum(event.quantity_milli) FROM inventory_transit_events event WHERE event.line_id=line.id AND event.kind='${kind}'`;

/** r46: the jsonb_array_elements_text membership fragment is shared by the
 *  attachment lot-image read and the calendar transit read. Only the expression
 *  varies; every caller keeps its own statement, binds and independent rows.
 *  The native command SQL mirror is deliberately left untouched. */
export const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;

/** r47: the reservations CTE head is shared by the executive metric and
 *  evidence reads; only the literal text is deduplicated and the head carries
 *  zero binds. Every caller keeps its own byte-identical tail, statement and
 *  binds. Locks, native transport, count adapters and r11-r46 helpers stay
 *  untouched. */
export const reservationsCteHeadSql = `WITH reservations AS (
  ${reservedWarehouseProductQtySql}
  FROM inventory_active_reservations GROUP BY warehouse_id,product_id
  )`;
/** r47: the positive marked-count EXISTS guard in the manufacturing and POS
 *  package-obligation reads shares one byte-identical literal; only the marked
 *  row source varies and the scope.full prefix stays outside. Every caller
 *  keeps its own statement, binds and tail. */
export const markedCountExistsSql = (rows: string) =>
  `c.company_id=scope.company_id AND EXISTS(SELECT 1 FROM (${rows}) marker WHERE marker.id=c.id)`;
/** r47: the command-receipt replay probe prefix is shared by the record-scoped
 *  purchase probe and the unscoped initial-count probe; only the trailing
 *  record_id and actor/company predicates vary, in their original bind order. */
export const replayProbePrefixSql = 'SELECT 1 FROM command_receipts WHERE command_id=? AND operation=?';

/** r53: the count-list projection head is byte-identical across the list, page
 *  and scoped-count reads. Only the literal text is shared; every caller keeps
 *  its own markers, rows, binds, guard/ORDER tails and independent statement.
 *  Locks, native transport, count sources/adapters and r11-r47 helpers stay
 *  untouched. */
export const countSelectHeadSql = 'SELECT c.*,w.name warehouse_name,';

/** r54: the three package-obligation stock heads share one byte-identical
 *  projection; only the kind literal varies. Only the SQL text is shared;
 *  every caller keeps its own statement, binds and guard/ORDER tail. */
export const packageObligationStockHeadSql = (kind: string) =>
  `SELECT '${kind}',concat(stock.warehouse_id,':',stock.product_id),to_jsonb(stock),stock.quantity_milli<0 OR stock.value_minor<0`;

/** r55: the sales quality-hold and supply transit-cancel purchase guards share
 *  one byte-identical command-receipt record-id head; only the record id varies.
 *  Only the SQL text is shared; every caller keeps its own guard/ORDER tail,
 *  binds and independent statement. Locks, native transport, count
 *  sources/adapters and r11-r54 helpers stay untouched. */
export const commandReceiptRecordIdGuardSql = (id: string) =>
  `SELECT 1 FROM command_receipts receipt WHERE receipt.record_id=${id}`;

/** r56: the commerce return-lot allocation read and the transfer-lot allocation
 *  read share one byte-identical, bind-free projection head. Only the literal
 *  text is shared; every caller keeps its own guard/projection tail, binds,
 *  statement and ORDER clause. Locks, native transport, count sources/adapters
 *  and r11-r55 helpers stay untouched. */
export const lotAllocationRowsHeadSql = 'SELECT a.*,l.lot_code,l.serial_number,l.expiry_date,';

/** r57: the quality-disposal count source's same-product line guard and its
 *  NOT-prefixed cross-product exclusion share one byte-identical, bind-free
 *  EXISTS head; only the closing tail varies. Every caller keeps its own NOT
 *  prefix, tail, binds and independent statement. Locks, native transport,
 *  count sources/adapters and r11-r56 helpers stay untouched. */
export const countLineProductGuardSql = (tail: string) =>
  `EXISTS(SELECT 1 FROM inventory_count_lines line WHERE line.count_id=c.id AND line.product_id${tail}`;

/** r58: the count-line and transfer-line detail reads share one byte-identical
 *  projection head. Only the literal text is shared; every caller keeps its own
 *  projection tail, table/join scope, bind and independent statement. Locks,
 *  native transport, count sources/adapters and r11-r57 helpers stay untouched. */
export const countLineStockHeadSql = 'SELECT l.*,p.sku,p.tracking_mode,';

/** r59: the manufacturing and PMS package-obligation stock rows share one
 *  byte-identical, bind-free projection head; only the projection tail, source
 *  scope and statement vary. Every caller keeps its own tail and independent
 *  statement. Locks, native transport, count sources/adapters and r11-r58
 *  helpers stay untouched. */
export const stockObligationHeadSql = "SELECT 'stock',concat(s.warehouse_id,':',s.product_id),";
