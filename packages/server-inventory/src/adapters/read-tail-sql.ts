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
