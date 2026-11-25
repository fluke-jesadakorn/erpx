import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { qualitySourceArmHead } from '../../../packages/server-quality/src/adapters/quality-source-arm-head';
import { qualitySourceRows } from '../../../packages/server-quality/src/adapters/quality-source-rows-sql';
import { evidenceHead, inspectionEvidenceSource, holdEvidenceSource, inspectionMetricSource } from '../../../packages/server-quality/src/adapters/inspection-evidence-source-sql';
import { hospitalityOwnerProjectionSources } from '../../../packages/server-quality/src/adapters/hospitality-owner-projection-sources';
import { QUALITY_HOLD_SOURCE_SELECT, inventoryExecutiveOwnerProjections } from '../../../packages/server-quality/src/adapters/inventory-executive-owner-projections';
import { collaborationNotificationSnapshotSources } from '../../../packages/server-quality/src/adapters/collaboration-notification-snapshot-sources';
import { inventoryQualityHoldReadSource } from '../../../packages/server-quality/src/adapters/inventory-hold-read-source';
import { qualityAttachmentReadSourceImage } from '../../../packages/server-quality/src/adapters/attachment-read-sources';
import { qualityHoldPurchaseSource, qualityPurchaseSourceRows } from '../../../packages/server-quality/src/adapters/purchase-sources';
import { executiveMainEvidenceSources } from '../../../packages/server-quality/src/adapters/executive-main-evidence-sources';
import { readExecutiveMetricProjection } from '../../../packages/server-quality/src/adapters/executive-metric-projections';
import { calendarQualityQualityFacts } from '../../../packages/server-quality/src/adapters/calendar-quality-facts';
import { qualityProductSourceEntry } from '../../../packages/server-quality/src/adapters/product-entry';

const srcRoot = resolve(__dirname, '../../../packages/server-quality/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');

// Pre-dedupe byte oracles captured from the r25 source before this pass.
// Every helper call below must reproduce these strings byte-for-byte.
const before = {
  "hospitality": {
    "inventory_quality_inspection_attachments": "SELECT \"inspection_id\",\"attachment_id\",\"ordinal\" FROM inventory_quality_inspection_attachments",
    "inventory_quality_inspections": "SELECT \"id\",\"hold_id\",\"reviewed_version\",\"result\",\"evidence\",\"inspected_by\",\"created_at\",\"spec_id\",\"spec_version\" FROM inventory_quality_inspections"
  },
  "holdSelect": "SELECT \"id\", \"company_id\", \"branch_id\", \"warehouse_id\", \"product_id\", \"reason\", \"due_date\", \"status\", \"created_by\", \"last_inspector\", \"last_result\", \"released_by\", \"release_reason\", \"released_at\", \"version\", \"created_at\", \"updated_at\", \"cancel_requested_by\", \"cancel_reason\", \"cancelled_by\", \"cancelled_at\", \"lot_id\", \"held_quantity_milli\", \"source_type\", \"source_id\", \"disposition_kind\", \"disposition_reason\", \"disposition_requested_by\", \"disposition_count_id\", \"disposed_by\", \"disposed_at\", \"spec_id\", \"spec_version\", \"quantity_scope\", \"disposition_quantity_milli\" FROM inventory_quality_holds",
  "notifications": {
    "quality_notifications_inventory_quality_holds_h_from": "FROM (SELECT \"id\", \"company_id\", \"branch_id\", \"warehouse_id\", \"product_id\", \"reason\", \"due_date\", \"status\", \"created_by\", \"last_inspector\", \"last_result\", \"released_by\", \"release_reason\", \"released_at\", \"version\", \"created_at\", \"updated_at\", \"cancel_requested_by\", \"cancel_reason\", \"cancelled_by\", \"cancelled_at\", \"lot_id\", \"held_quantity_milli\", \"source_type\", \"source_id\", \"disposition_kind\", \"disposition_reason\", \"disposition_requested_by\", \"disposition_count_id\", \"disposed_by\", \"disposed_at\", \"spec_id\", \"spec_version\", \"quantity_scope\", \"disposition_quantity_milli\" FROM inventory_quality_holds) h",
    "quality_notifications_inventory_quality_alerts_a_from": "FROM (SELECT \"id\", \"hold_id\", \"phase\", \"source_version\", \"due_date\", \"detected_at\", \"version\", \"acknowledged_by\", \"acknowledged_at\", \"reason\" FROM inventory_quality_alerts) a",
    "quality_notifications_inventory_quality_holds_h_join": "JOIN (SELECT \"id\", \"company_id\", \"branch_id\", \"warehouse_id\", \"product_id\", \"reason\", \"due_date\", \"status\", \"created_by\", \"last_inspector\", \"last_result\", \"released_by\", \"release_reason\", \"released_at\", \"version\", \"created_at\", \"updated_at\", \"cancel_requested_by\", \"cancel_reason\", \"cancelled_by\", \"cancelled_at\", \"lot_id\", \"held_quantity_milli\", \"source_type\", \"source_id\", \"disposition_kind\", \"disposition_reason\", \"disposition_requested_by\", \"disposition_count_id\", \"disposed_by\", \"disposed_at\", \"spec_id\", \"spec_version\", \"quantity_scope\", \"disposition_quantity_milli\" FROM inventory_quality_holds) h"
  },
  "inventoryHoldRead": "FROM (SELECT warehouse_id, product_id, lot_id, status FROM inventory_quality_holds) h",
  "specs": "SELECT s.id,s.company_id,jsonb_build_object('specification',to_jsonb(s),'metrics',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.id) FROM inventory_quality_spec_metrics m WHERE m.spec_id=s.id)) image FROM inventory_quality_specs s"
};

const calls: { sql: string; binds: unknown[] }[] = [];
const db = {
  prepare(sql: string) {
    const rec = { sql, binds: [] as unknown[] };
    calls.push(rec);
    const stmt: any = {
      bind(...args: unknown[]) { rec.binds = args; return stmt; },
      all() { return Promise.resolve({ results: [] }); },
      first() { return Promise.resolve(null); },
      run() { return Promise.resolve(undefined); },
    };
    return stmt;
  },
} as any;
const capture = async (fn: () => unknown) => {
  calls.length = 0;
  try { await fn(); } catch { /* guard-only readers stop at the first empty first() */ }
  return calls.map((call) => call.sql);
};

describe('quality read-tail r26 closed projection family (P)', () => {
  test('the six closed sources reproduce their pre-dedupe literals byte-for-byte', async () => {
    expect(hospitalityOwnerProjectionSources.inventory_quality_inspection_attachments).toBe(before.hospitality.inventory_quality_inspection_attachments);
    expect(hospitalityOwnerProjectionSources.inventory_quality_inspections).toBe(before.hospitality.inventory_quality_inspections);
    expect(QUALITY_HOLD_SOURCE_SELECT).toBe(before.holdSelect);
    expect(inventoryExecutiveOwnerProjections.inventory_quality_holds).toBe(before.holdSelect);
    expect(collaborationNotificationSnapshotSources().quality_notifications_inventory_quality_holds_h_from).toBe(`FROM (${before.holdSelect}) h`);
    expect(collaborationNotificationSnapshotSources().quality_notifications_inventory_quality_alerts_a_from).toBe(before.notifications.quality_notifications_inventory_quality_alerts_a_from);
    expect(collaborationNotificationSnapshotSources().quality_notifications_inventory_quality_holds_h_join).toBe(`JOIN (${before.holdSelect}) h`);
    expect(inventoryQualityHoldReadSource()).toBe(before.inventoryHoldRead);
    expect(await qualityAttachmentReadSourceImage(db, 'c', 'inventory_quality_spec', 'id')).toBeNull();
    expect(calls[0].sql).toContain(`(${before.specs}) s WHERE s.id=? AND s.company_id=?`);
  });
  test('none of the closed sources or the shared head carries a bind or a lock', () => {
    for (const sql of Object.values(before.hospitality)) {
      expect(sql.startsWith('SELECT ')).toBe(true);
      expect(sql).not.toContain('?');
    }
    expect(before.holdSelect).not.toContain('?');
    expect(before.notifications.quality_notifications_inventory_quality_alerts_a_from).not.toContain('?');
    expect(before.inventoryHoldRead).not.toContain('?');
    expect(qualitySourceArmHead('x', 'y')).toBe('SELECT x FROM y');
    expect(qualitySourceArmHead('x', 'y', ' WHERE z=?')).toBe('SELECT x FROM y WHERE z=?');
    expect(qualitySourceArmHead('x', 'y')).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
  test('all six call sites route through qualitySourceArmHead with no residual inline copy', () => {
    const files = [
      'adapters/hospitality-owner-projection-sources.ts',
      'adapters/inventory-executive-owner-projections.ts',
      'adapters/collaboration-notification-snapshot-sources.ts',
      'adapters/inventory-hold-read-source.ts',
      'adapters/attachment-read-sources.ts',
    ];
    for (const file of files) expect(read(file)).toContain('qualitySourceArmHead(');
    expect(read('adapters/hospitality-owner-projection-sources.ts').match(/qualitySourceArmHead\(/g)).toHaveLength(2);
    expect(read('adapters/inventory-executive-owner-projections.ts').match(/qualitySourceArmHead\(/g)).toHaveLength(1);
    expect(read('adapters/inventory-executive-owner-projections.ts')).not.toContain('QUALITY_HOLD_SOURCE_SELECT = "SELECT');
    expect(read('adapters/collaboration-notification-snapshot-sources.ts').match(/qualitySourceArmHead\(/g)).toHaveLength(1);
    expect(read('adapters/inventory-hold-read-source.ts').match(/qualitySourceArmHead\(/g)).toHaveLength(1);
    expect(read('adapters/attachment-read-sources.ts').match(/qualitySourceArmHead\(/g)).toHaveLength(1);
    expect(read('adapters/attachment-read-sources.ts')).not.toContain('const specs=`SELECT');
    expect(read('adapters/quality-source-arm-head.ts').match(/SELECT /g) ?? []).toHaveLength(0);
  });
});

describe('quality read-tail r26 point-read family (Q)', () => {
  test('the qualitySourceRows head reproduces the held point read with binds untouched', async () => {
    expect(qualitySourceRows('inventory_quality_holds')).toBe('SELECT * FROM inventory_quality_holds');
    expect(read('adapters/purchase-sources.ts')).toContain('${qualitySourceRows("inventory_quality_holds")} WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?)');
    const [sql] = await capture(() => qualityHoldPurchaseSource(db, { companyId: 'c', branchId: null }, 'id'));
    expect(sql).toBe('SELECT * FROM inventory_quality_holds WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?)');
  });
  test('evidenceHead reproduces the spec, replay and scope point reads byte-for-byte', () => {
    expect(qualitySourceArmHead('id,company_id,product_id,created_by', 'inventory_quality_specs', ' WHERE id=? AND company_id=?'))
      .toBe('SELECT id,company_id,product_id,created_by FROM inventory_quality_specs WHERE id=? AND company_id=?');
    expect(read('adapters/native_quality_read_sql.rs')).toContain('"SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>\'companyId\'=?"');
    expect(evidenceHead('1', ' FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope', ' WHERE ${currentScope.where}'))
      .toBe('SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${currentScope.where}');
    // Exact call-text oracles: a duplicated relation or predicate would still
    // typecheck and would only surface as a changed prepared statement.
    expect(read('adapters/attachment-read.ts')).toContain("evidenceHead('1',` FROM${qualityWorkScopeTuple(\"?::text company_id,?::text branch_id,?::text department_id\",\"workforce_actor_scope\",\"\")}`,` WHERE ${currentScope.where}`)");
    expect(read('adapters/purchases.ts')).toContain('qualitySourceArmHead("id,company_id,product_id,created_by","inventory_quality_specs"," WHERE id=? AND company_id=?")');
    expect(read('adapters/purchases.ts')).toContain('nativeQualityReadFirst(RD_RECEIPT_REPLAY,[command.key,command.operation,command.target.id,actor.userId,actor.companyId],database)');
    expect(read('adapters/purchases.ts')).toContain('evidenceHead(`${assignmentReadSql("quality_scope",permission)} allowed`,` FROM ${qualityWorkScopeTuple("?::text company_id,?::text branch_id,NULL::text department_id","quality_scope")}`,\'\')');
    expect(read('adapters/attachment-read.ts')).not.toContain('`SELECT 1 FROM${qualityWorkScopeTuple(');
    expect(read('adapters/purchases.ts')).not.toContain('"SELECT id,company_id,product_id,created_by FROM inventory_quality_specs');
    expect(read('adapters/purchases.ts')).not.toContain('"SELECT 1 FROM command_receipts');
    expect(read('adapters/purchases.ts').match(/qualitySourceArmHead\(/g)).toHaveLength(1);
    expect(read('adapters/purchases.ts').match(/evidenceHead\(/g)).toHaveLength(1);
    expect(read('adapters/quality-source-rows-sql.ts').match(/SELECT /g) ?? []).toHaveLength(0);
  });
});

describe('quality read-tail r26 evidence-head family (R)', () => {
  test('exported evidenceHead reproduces the pre-dedupe full heads with exact spacing', () => {
    expect(evidenceHead('i.hold_id', '\n    FROM inventory_quality_inspections i\n    JOIN inventory_quality_holds h ON h.id=i.hold_id\n    WHERE i.id=? AND h.company_id=? AND (?::text IS NULL OR h.branch_id=?)\n      AND i.created_at>=? AND i.created_at<?', ''))
      .toBe('SELECT i.hold_id\n    FROM inventory_quality_inspections i\n    JOIN inventory_quality_holds h ON h.id=i.hold_id\n    WHERE i.id=? AND h.company_id=? AND (?::text IS NULL OR h.branch_id=?)\n      AND i.created_at>=? AND i.created_at<?');
    expect(read('adapters/inspection-evidence-source-sql.ts')).toContain('export const evidenceHead =');
    expect(read('adapters/inspection-evidence-source-sql.ts').match(/SELECT /g)).toHaveLength(1);
    for (const file of ['alerts', 'executive-main-evidence-sources', 'executive-metric-projections', 'calendar-quality-facts', 'attachment-read-sources', 'product-entry', 'purchase-sources', 'supply-package-obligations']) {
      expect(read(`adapters/${file}.ts`)).toContain('evidenceHead(');
    }
    // Exact call-text oracles for the heads whose enclosing readers need a live
    // transaction (alerts) or a settlement lock (supply) and cannot be replayed
    // through a bare prepare recorder.
    expect(read('adapters/alerts.ts')).toContain('evidenceHead(`a.*,h.company_id,h.branch_id,h.status,h.version hold_version,p.name product_name,w.name warehouse_name,');
    expect(read('adapters/alerts.ts')).toContain('`,` FROM inventory_quality_alerts a\n         JOIN inventory_quality_holds h ON h.id=a.hold_id');
    expect(read('adapters/supply-package-obligations.ts')).toContain('${evidenceHead(`hold.*,source.kind source_kind,source.witness`,` FROM inventory_quality_holds hold CROSS JOIN scope');
    expect(read('adapters/attachment-read-sources.ts')).toContain('evidenceHead(`jsonb_build_object(');
    expect(read('adapters/attachment-read-sources.ts')).toContain('` FROM inventory_quality_holds h LEFT JOIN (${qualityPurchaseSourceRows}) source');
  });
  test('composed reader SQL stays byte-identical to the pre-dedupe captured statements', async () => {
    const actor = { companyId: 'c', branchId: 'b' } as any;
    const filters = { from: '2026-01-01', to: '2026-01-31', currency: 'USD' } as any;
    const evidence = executiveMainEvidenceSources(db);
    expect(await capture(() => evidence.factsSearchQualityInspectionEvidence({ filters, query: 'q', limit: 1, metric: 'inspection-pass-rate' }, actor, null))).toEqual(["SELECT i.id,i.hold_id,i.result,i.created_at,i.reviewed_version,p.name product_name,w.name warehouse_name\n    FROM inventory_quality_inspections i\n    JOIN inventory_quality_holds h ON h.id=i.hold_id\n    JOIN (SELECT \"id\",\"company_id\",\"sku\",\"name\",\"unit\",\"price_minor\",\"tax_bps\",\"status\",\"version\",\"tracking_mode\" FROM commerce_products) p ON p.id=h.product_id AND p.company_id=h.company_id\n    JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"code\",\"name\",\"status\",\"version\" FROM commerce_warehouses) w ON w.id=h.warehouse_id AND w.company_id=h.company_id\n    WHERE h.company_id=? AND (?::text IS NULL OR h.branch_id=?) AND i.created_at>=? AND i.created_at<? AND (position(lower(?) in lower(p.name))>0 OR position(lower(?) in lower(i.result))>0)\n    ORDER BY i.created_at DESC,i.id DESC LIMIT ?"]);
    expect(await capture(() => evidence.factsSearchQualityHoldEvidence(true, { filters, query: 'q', limit: 1, metric: 'quality-holds' }, actor, null))).toEqual(["SELECT h.id,h.id hold_id,h.released_at created_at,h.version reviewed_version,h.status result,\n      p.name product_name,w.name warehouse_name\n    FROM inventory_quality_holds h\n    JOIN (SELECT \"id\",\"company_id\",\"sku\",\"name\",\"unit\",\"price_minor\",\"tax_bps\",\"status\",\"version\",\"tracking_mode\" FROM commerce_products) p ON p.id=h.product_id AND p.company_id=h.company_id\n    JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"code\",\"name\",\"status\",\"version\" FROM commerce_warehouses) w ON w.id=h.warehouse_id AND w.company_id=h.company_id\n    WHERE h.company_id=? AND (?::text IS NULL OR h.branch_id=?) AND h.status='released' AND h.released_at>=? AND h.released_at<? AND (position(lower(?) in lower(p.name))>0 OR position(lower(?) in lower(h.status))>0)\n    ORDER BY h.released_at DESC,h.id DESC LIMIT ?"]);
    expect(await capture(() => readExecutiveMetricProjection(db, actor, 'quality-holds', filters, 'branch'))).toEqual(["SELECT COALESCE((SELECT n.display_name FROM (SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes) n WHERE n.id=h.branch_id AND n.company_id=h.company_id),'Company-wide') label,count(*)::text value FROM inventory_quality_holds h WHERE h.company_id=? AND (?::text IS NULL OR h.branch_id=?) AND h.status NOT IN ('released','cancelled','disposed') GROUP BY 1 ORDER BY 1"]);
    expect(await capture(() => readExecutiveMetricProjection(db, actor, 'inspection-pass-rate', filters, 'branch'))).toEqual(["SELECT COALESCE((SELECT n.display_name FROM (SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes) n WHERE n.id=h.branch_id AND n.company_id=h.company_id),'Company-wide') label,\n        count(*) FILTER (WHERE i.result='pass')::text numerator,count(*)::text denominator\n      FROM inventory_quality_inspections i JOIN inventory_quality_holds h ON h.id=i.hold_id\n      WHERE h.company_id=? AND (?::text IS NULL OR h.branch_id=?) AND i.created_at>=? AND i.created_at<?\n      GROUP BY 1 ORDER BY 1"]);
    expect(await capture(() => calendarQualityQualityFacts(db, actor).read1({ value1: 'c', value2: null, value3: 'c', value4: '2026-01-01', value5: '2026-01-31', value6: 'mine', value7: 'u', value8: null, value9: 'x', value10: 1 } as any))).toEqual(["SELECT h.id,h.due_date,h.status,h.version,h.branch_id,p.name product_name FROM inventory_quality_holds h JOIN (SELECT \"id\",\"company_id\",\"sku\",\"name\",\"unit\",\"price_minor\",\"tax_bps\",\"status\",\"version\",\"tracking_mode\" FROM commerce_products) p ON p.id=h.product_id WHERE h.company_id=? AND (?::text IS NULL OR h.branch_id=?) AND h.branch_id IN (SELECT jsonb_array_elements_text(?::jsonb)) AND h.due_date>=? AND h.due_date<? AND h.status NOT IN ('released','cancelled','disposed') AND (?='team' OR h.created_by=? OR h.last_inspector=?) AND (?::text IS NULL OR h.created_by IN (SELECT user_id FROM (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) hr_people WHERE id=? AND company_id=h.company_id)) AND (?='' OR p.name ILIKE '%'||?||'%')  ORDER BY h.id LIMIT ?"]);
    expect(await capture(() => qualityProductSourceEntry(db, { companyId: 'c', branchId: null, departmentId: null, userId: 'u', assignmentId: 'a', assignmentVersion: 1, role: 'r' }, { kind: 'hold', id: 'id' }))).toEqual(["WITH quality_entry_clock AS MATERIALIZED (SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms)\n SELECT quality_entry.kind FROM (SELECT s.*,NULL::text department_id FROM (WITH RECURSIVE lineage AS (\n SELECT id hold_id,id root_id,0 depth FROM inventory_quality_holds\n UNION ALL SELECT walk.hold_id,parent.id,walk.depth+1 FROM lineage walk JOIN inventory_quality_holds child ON child.id=walk.root_id JOIN inventory_quality_holds parent ON parent.company_id=child.company_id AND parent.branch_id=child.branch_id AND parent.warehouse_id=child.warehouse_id AND parent.product_id=child.product_id AND parent.lot_id IS NOT DISTINCT FROM child.lot_id AND parent.created_by=child.created_by AND parent.spec_id IS NOT DISTINCT FROM child.spec_id AND parent.spec_version IS NOT DISTINCT FROM child.spec_version AND parent.source_type IS NOT DISTINCT FROM child.source_type AND parent.status='disposed'\n JOIN inventory_counts c ON c.id=parent.disposition_count_id AND c.quality_hold_id=parent.id AND c.company_id=parent.company_id AND c.branch_id=parent.branch_id AND c.warehouse_id=parent.warehouse_id AND c.status='posted'\n JOIN inventory_quality_disposition_events e ON e.hold_id=parent.id AND e.count_id=c.id AND e.action='post'\n WHERE walk.depth<64 AND child.id=parent.id||'-remaining-'||c.id AND child.source_id=parent.source_id||':remaining:'||c.id AND child.held_quantity_milli=parent.held_quantity_milli-parent.disposition_quantity_milli AND child.held_quantity_milli>0\n ), roots AS (SELECT DISTINCT ON(hold_id) hold_id,root_id FROM lineage ORDER BY hold_id,depth DESC)\n SELECT h.id,h.company_id,h.branch_id,h.warehouse_id,h.product_id,h.lot_id,root.id root_id,\n COALESCE(origin.kind,'advanced') kind,CASE WHEN origin.kind='customer-return' THEN 'inventory.stock-core' ELSE 'supply.stock' END stock_capability,\n CASE WHEN origin.kind='customer-return' THEN 'finance.sales-posting' ELSE 'finance.inventory-valuation' END finance_capability,\n jsonb_build_array(jsonb_build_object('id',h.id,'company',h.company_id,'branch',h.branch_id,'warehouse',h.warehouse_id,'product',h.product_id,'lot',h.lot_id,'sourceType',h.source_type,'sourceId',h.source_id,'spec',h.spec_id,'specVersion',h.spec_version,'creator',h.created_by,'quantity',h.held_quantity_milli),jsonb_build_object('id',root.id,'company',root.company_id,'branch',root.branch_id,'warehouse',root.warehouse_id,'product',root.product_id,'lot',root.lot_id,'sourceType',root.source_type,'sourceId',root.source_id,'spec',root.spec_id,'specVersion',root.spec_version,'creator',root.created_by,'quantity',root.held_quantity_milli),to_jsonb(origin)) witness\n FROM roots JOIN inventory_quality_holds h ON h.id=roots.hold_id JOIN inventory_quality_holds root ON root.id=roots.root_id\n LEFT JOIN (SELECT 'receipt'::text kind,a.quality_hold_id hold_id,o.company_id,o.branch_id,o.warehouse_id,ol.product_id,a.lot_id,a.id source_id,o.id owner_id,a.quantity_milli,a.value_minor,NULL::text created_by\n FROM inventory_fulfillment_lot_allocations a JOIN commerce_fulfillment_lines line ON line.id=a.fulfillment_line_id\n JOIN commerce_order_lines ol ON ol.id=line.order_line_id AND ol.item_type<>'service'\n JOIN commerce_fulfillments f ON f.id=line.fulfillment_id JOIN commerce_orders o ON o.id=f.order_id AND o.id=ol.order_id AND o.kind='purchase'\n WHERE a.quality_hold_id IS NOT NULL AND a.disposition='quarantine' UNION ALL SELECT 'customer-return'::text kind,'quality-return-'||receipt.command_id||'-'||line.ordinal::text hold_id,r.company_id,r.branch_id,o.warehouse_id,line.product_id,NULL::text lot_id,line.id source_id,r.id owner_id,line.quantity_milli,line.value_minor,r.approved_by created_by FROM commerce_returns r JOIN commerce_orders o ON o.id=r.order_id AND o.company_id=r.company_id AND o.kind='sales'\n JOIN commerce_fulfillments f ON f.id=r.fulfillment_id AND f.order_id=o.id AND f.invoice_id=r.invoice_id\n JOIN commerce_return_lines line ON line.return_id=r.id JOIN commerce_fulfillment_lines fl ON fl.id=line.fulfillment_line_id AND fl.fulfillment_id=f.id\n JOIN commerce_order_lines ol ON ol.id=fl.order_line_id AND ol.order_id=o.id AND ol.product_id=line.product_id AND ol.item_type<>'service'\n JOIN command_receipts receipt ON receipt.record_id=r.id AND receipt.operation='commerce.return.approve' AND receipt.actor_id=r.approved_by AND receipt.authority_json::jsonb->>'companyId'=r.company_id\n WHERE r.kind='customer_return' AND r.status IN ('approved','posted') AND NOT EXISTS(SELECT 1 FROM commerce_return_lot_allocations allocation WHERE allocation.return_line_id=line.id) UNION ALL SELECT 'customer-return'::text kind,'quality-return-'||receipt.command_id||'-'||line.ordinal::text||'-'||allocation.lot_ordinal::text hold_id,r.company_id,r.branch_id,o.warehouse_id,line.product_id,allocation.lot_id,allocation.id source_id,r.id owner_id,allocation.quantity_milli,allocation.value_minor,r.approved_by created_by FROM commerce_returns r JOIN commerce_orders o ON o.id=r.order_id AND o.company_id=r.company_id AND o.kind='sales'\n JOIN commerce_fulfillments f ON f.id=r.fulfillment_id AND f.order_id=o.id AND f.invoice_id=r.invoice_id\n JOIN commerce_return_lines line ON line.return_id=r.id JOIN commerce_fulfillment_lines fl ON fl.id=line.fulfillment_line_id AND fl.fulfillment_id=f.id\n JOIN commerce_order_lines ol ON ol.id=fl.order_line_id AND ol.order_id=o.id AND ol.product_id=line.product_id AND ol.item_type<>'service'\n JOIN command_receipts receipt ON receipt.record_id=r.id AND receipt.operation='commerce.return.approve' AND receipt.actor_id=r.approved_by AND receipt.authority_json::jsonb->>'companyId'=r.company_id\n JOIN (SELECT *,row_number() OVER (PARTITION BY return_line_id ORDER BY id)-1 lot_ordinal FROM commerce_return_lot_allocations) allocation ON allocation.return_line_id=line.id\n JOIN inventory_fulfillment_lot_allocations original ON original.id=allocation.fulfillment_allocation_id AND original.fulfillment_line_id=fl.id AND original.lot_id=allocation.lot_id\n WHERE r.kind='customer_return' AND r.status IN ('approved','posted')) origin ON root.spec_id IS NULL AND origin.hold_id=root.id AND origin.company_id=root.company_id AND origin.branch_id=root.branch_id AND origin.warehouse_id=root.warehouse_id AND origin.product_id=root.product_id AND origin.lot_id IS NOT DISTINCT FROM root.lot_id AND origin.quantity_milli=root.held_quantity_milli\n AND ((origin.kind='receipt' AND root.source_type='goods_receipt') OR (origin.kind='customer-return' AND root.source_type='customer_return' AND origin.source_id=root.source_id AND origin.created_by=root.created_by))) s) quality_entry CROSS JOIN quality_entry_clock\n WHERE quality_entry.id=? AND quality_entry.company_id=? AND (?::text IS NULL OR quality_entry.branch_id=?)\n AND EXISTS(SELECT 1 FROM assignments g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=quality_entry.company_id\n    AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n    AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n    AND iam_role_allows(g.role,g.company_id,'quality.read')\n    AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=quality_entry.branch_id OR g.scope_kind='department' AND g.scope_node_id=quality_entry.department_id)) AND EXISTS(SELECT 1 FROM assignments WHERE id=? AND version=?)\n AND TRUE\n AND ((quality_entry.kind='advanced' AND ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=quality_entry.company_id AND starts_at<=quality_entry_clock.now_ms AND state<>'inactive'\n      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=quality_entry.company_id AND retained_at<=quality_entry_clock.now_ms\n        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='supply-quality' AND purchase.package_version=1) AND EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=quality_entry.company_id AND starts_at<=quality_entry_clock.now_ms AND state<>'inactive'\n      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=quality_entry.company_id AND retained_at<=quality_entry_clock.now_ms\n        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') parent WHERE parent.package_id='supply' AND parent.package_version IN (1))))) OR (quality_entry.kind='receipt' AND ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=quality_entry.company_id AND starts_at<=quality_entry_clock.now_ms AND state<>'inactive'\n      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=quality_entry.company_id AND retained_at<=quality_entry_clock.now_ms\n        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='supply' AND purchase.package_version=1))) AND ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=quality_entry.company_id AND starts_at<=quality_entry_clock.now_ms AND state<>'inactive'\n      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=quality_entry.company_id AND retained_at<=quality_entry_clock.now_ms\n        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='supply' AND purchase.package_version=1)))) OR (quality_entry.kind='customer-return' AND ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=quality_entry.company_id AND starts_at<=quality_entry_clock.now_ms AND state<>'inactive'\n      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=quality_entry.company_id AND retained_at<=quality_entry_clock.now_ms\n        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='sales' AND purchase.package_version=1))) AND ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=quality_entry.company_id AND starts_at<=quality_entry_clock.now_ms AND state<>'inactive'\n      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=quality_entry.company_id AND retained_at<=quality_entry_clock.now_ms\n        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='pos' AND purchase.package_version=1)) OR (EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=quality_entry.company_id AND starts_at<=quality_entry_clock.now_ms AND state<>'inactive'\n      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=quality_entry.company_id AND retained_at<=quality_entry_clock.now_ms\n        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='pms' AND purchase.package_version=1)) OR (EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=quality_entry.company_id AND starts_at<=quality_entry_clock.now_ms AND state<>'inactive'\n      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=quality_entry.company_id AND retained_at<=quality_entry_clock.now_ms\n        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='sales' AND purchase.package_version=1)) OR (EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=quality_entry.company_id AND starts_at<=quality_entry_clock.now_ms AND state<>'inactive'\n      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=quality_entry.company_id AND retained_at<=quality_entry_clock.now_ms\n        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='manufacturing' AND purchase.package_version=1)))))"]);
    expect(qualityPurchaseSourceRows).toBe("WITH RECURSIVE lineage AS (\n SELECT id hold_id,id root_id,0 depth FROM inventory_quality_holds\n UNION ALL SELECT walk.hold_id,parent.id,walk.depth+1 FROM lineage walk JOIN inventory_quality_holds child ON child.id=walk.root_id JOIN inventory_quality_holds parent ON parent.company_id=child.company_id AND parent.branch_id=child.branch_id AND parent.warehouse_id=child.warehouse_id AND parent.product_id=child.product_id AND parent.lot_id IS NOT DISTINCT FROM child.lot_id AND parent.created_by=child.created_by AND parent.spec_id IS NOT DISTINCT FROM child.spec_id AND parent.spec_version IS NOT DISTINCT FROM child.spec_version AND parent.source_type IS NOT DISTINCT FROM child.source_type AND parent.status='disposed'\n JOIN inventory_counts c ON c.id=parent.disposition_count_id AND c.quality_hold_id=parent.id AND c.company_id=parent.company_id AND c.branch_id=parent.branch_id AND c.warehouse_id=parent.warehouse_id AND c.status='posted'\n JOIN inventory_quality_disposition_events e ON e.hold_id=parent.id AND e.count_id=c.id AND e.action='post'\n WHERE walk.depth<64 AND child.id=parent.id||'-remaining-'||c.id AND child.source_id=parent.source_id||':remaining:'||c.id AND child.held_quantity_milli=parent.held_quantity_milli-parent.disposition_quantity_milli AND child.held_quantity_milli>0\n ), roots AS (SELECT DISTINCT ON(hold_id) hold_id,root_id FROM lineage ORDER BY hold_id,depth DESC)\n SELECT h.id,h.company_id,h.branch_id,h.warehouse_id,h.product_id,h.lot_id,root.id root_id,\n COALESCE(origin.kind,'advanced') kind,CASE WHEN origin.kind='customer-return' THEN 'inventory.stock-core' ELSE 'supply.stock' END stock_capability,\n CASE WHEN origin.kind='customer-return' THEN 'finance.sales-posting' ELSE 'finance.inventory-valuation' END finance_capability,\n jsonb_build_array(jsonb_build_object('id',h.id,'company',h.company_id,'branch',h.branch_id,'warehouse',h.warehouse_id,'product',h.product_id,'lot',h.lot_id,'sourceType',h.source_type,'sourceId',h.source_id,'spec',h.spec_id,'specVersion',h.spec_version,'creator',h.created_by,'quantity',h.held_quantity_milli),jsonb_build_object('id',root.id,'company',root.company_id,'branch',root.branch_id,'warehouse',root.warehouse_id,'product',root.product_id,'lot',root.lot_id,'sourceType',root.source_type,'sourceId',root.source_id,'spec',root.spec_id,'specVersion',root.spec_version,'creator',root.created_by,'quantity',root.held_quantity_milli),to_jsonb(origin)) witness\n FROM roots JOIN inventory_quality_holds h ON h.id=roots.hold_id JOIN inventory_quality_holds root ON root.id=roots.root_id\n LEFT JOIN (SELECT 'receipt'::text kind,a.quality_hold_id hold_id,o.company_id,o.branch_id,o.warehouse_id,ol.product_id,a.lot_id,a.id source_id,o.id owner_id,a.quantity_milli,a.value_minor,NULL::text created_by\n FROM inventory_fulfillment_lot_allocations a JOIN commerce_fulfillment_lines line ON line.id=a.fulfillment_line_id\n JOIN commerce_order_lines ol ON ol.id=line.order_line_id AND ol.item_type<>'service'\n JOIN commerce_fulfillments f ON f.id=line.fulfillment_id JOIN commerce_orders o ON o.id=f.order_id AND o.id=ol.order_id AND o.kind='purchase'\n WHERE a.quality_hold_id IS NOT NULL AND a.disposition='quarantine' UNION ALL SELECT 'customer-return'::text kind,'quality-return-'||receipt.command_id||'-'||line.ordinal::text hold_id,r.company_id,r.branch_id,o.warehouse_id,line.product_id,NULL::text lot_id,line.id source_id,r.id owner_id,line.quantity_milli,line.value_minor,r.approved_by created_by FROM commerce_returns r JOIN commerce_orders o ON o.id=r.order_id AND o.company_id=r.company_id AND o.kind='sales'\n JOIN commerce_fulfillments f ON f.id=r.fulfillment_id AND f.order_id=o.id AND f.invoice_id=r.invoice_id\n JOIN commerce_return_lines line ON line.return_id=r.id JOIN commerce_fulfillment_lines fl ON fl.id=line.fulfillment_line_id AND fl.fulfillment_id=f.id\n JOIN commerce_order_lines ol ON ol.id=fl.order_line_id AND ol.order_id=o.id AND ol.product_id=line.product_id AND ol.item_type<>'service'\n JOIN command_receipts receipt ON receipt.record_id=r.id AND receipt.operation='commerce.return.approve' AND receipt.actor_id=r.approved_by AND receipt.authority_json::jsonb->>'companyId'=r.company_id\n WHERE r.kind='customer_return' AND r.status IN ('approved','posted') AND NOT EXISTS(SELECT 1 FROM commerce_return_lot_allocations allocation WHERE allocation.return_line_id=line.id) UNION ALL SELECT 'customer-return'::text kind,'quality-return-'||receipt.command_id||'-'||line.ordinal::text||'-'||allocation.lot_ordinal::text hold_id,r.company_id,r.branch_id,o.warehouse_id,line.product_id,allocation.lot_id,allocation.id source_id,r.id owner_id,allocation.quantity_milli,allocation.value_minor,r.approved_by created_by FROM commerce_returns r JOIN commerce_orders o ON o.id=r.order_id AND o.company_id=r.company_id AND o.kind='sales'\n JOIN commerce_fulfillments f ON f.id=r.fulfillment_id AND f.order_id=o.id AND f.invoice_id=r.invoice_id\n JOIN commerce_return_lines line ON line.return_id=r.id JOIN commerce_fulfillment_lines fl ON fl.id=line.fulfillment_line_id AND fl.fulfillment_id=f.id\n JOIN commerce_order_lines ol ON ol.id=fl.order_line_id AND ol.order_id=o.id AND ol.product_id=line.product_id AND ol.item_type<>'service'\n JOIN command_receipts receipt ON receipt.record_id=r.id AND receipt.operation='commerce.return.approve' AND receipt.actor_id=r.approved_by AND receipt.authority_json::jsonb->>'companyId'=r.company_id\n JOIN (SELECT *,row_number() OVER (PARTITION BY return_line_id ORDER BY id)-1 lot_ordinal FROM commerce_return_lot_allocations) allocation ON allocation.return_line_id=line.id\n JOIN inventory_fulfillment_lot_allocations original ON original.id=allocation.fulfillment_allocation_id AND original.fulfillment_line_id=fl.id AND original.lot_id=allocation.lot_id\n WHERE r.kind='customer_return' AND r.status IN ('approved','posted')) origin ON root.spec_id IS NULL AND origin.hold_id=root.id AND origin.company_id=root.company_id AND origin.branch_id=root.branch_id AND origin.warehouse_id=root.warehouse_id AND origin.product_id=root.product_id AND origin.lot_id IS NOT DISTINCT FROM root.lot_id AND origin.quantity_milli=root.held_quantity_milli\n AND ((origin.kind='receipt' AND root.source_type='goods_receipt') OR (origin.kind='customer-return' AND root.source_type='customer_return' AND origin.source_id=root.source_id AND origin.created_by=root.created_by))");
  });
  test('the r1-r25 helpers and protected fragment guards stay untouched', () => {
    expect(read('adapters/purchase-sources.ts')).toContain('qualityPurchaseSourceReadLocks');
    expect(read('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_specs');
    expect(read('adapters/sales-package-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_inspections,inventory_quality_disposition_events,inventory_quality_cancellations,inventory_quality_specs,inventory_quality_spec_metrics IN SHARE MODE');
    expect(read('adapters/supply-package-obligations.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_inspections,inventory_quality_cancellations');
    expect(read('adapters/attachments-service-facts.ts')).toContain('UPDATE ${descriptor.table} SET version=version+1');
    const calendar = read('adapters/calendar-quality-facts.ts');
    expect(calendar).toContain("import {qualityClockCte,scopeSql} from \"@erpx/server-quality/adapters/quality-scope-sql\";");
    expect(calendar).toContain("IN ${scopeSql('jsonb_array_elements_text(?::jsonb)')}");
    expect(calendar).toContain('EXISTS${scopeSql(`1 FROM (${qualityPurchaseSourceRows}) quality_calendar_source');
    expect(calendar).not.toContain('EXISTS(SELECT 1 FROM (${qualityPurchaseSourceRows})');
    expect(calendar).toContain('EXISTS${scopeSql(`1 FROM ${qualityWorkScopeTuple(');
    expect(read('adapters/executive-metric-projections.ts')).toContain('qualityBranchDisplayLabelSql');
    const entry = read('adapters/product-entry.ts');
    expect(entry).toContain("AND EXISTS${scopeSql('1 FROM assignments WHERE id=? AND version=?')}");
    expect(entry).not.toContain('AND EXISTS(SELECT 1 FROM assignments WHERE id=? AND version=?)');
    const purchase = read('adapters/attachment-purchases.ts');
    expect(purchase).toContain('EXISTS${scopeSql(`1 FROM (${source(scope.kind)}) quality_file_source');
    expect(purchase).not.toContain('EXISTS(SELECT 1 FROM (${source(scope.kind)})');
    const recorded = read('adapters/record-purchases.ts');
    expect(recorded).toContain('import {qualityClockCte,withScopeSql} from "@erpx/server-quality/adapters/quality-scope-sql";');
    expect(recorded).toContain('EXISTS${withScopeSql(`${qualityClockCte("quality_source_clock")} ${evidenceHead(\'1\',` FROM (${native}) quality_source');
    expect(recorded).not.toContain('EXISTS(${qualityClockCte("quality_source_clock")} SELECT 1 FROM');
    expect(read('adapters/customer_return_hold_statement.rs')).toContain('customer_return_hold_statements');
    const counts: Record<string, number> = {
      'quality-work-scope-sql': 0, 'native-source-wrapper-sql': 0, 'inspection-evidence-source-sql': 1,
      'quality-lineage-recursion-sql': 0, 'quality-child-agg-sql': 0,
      'quality-receipt-guard-sql': 0, 'quality-source-rows-sql': 0, 'quality-scope-sql': 0,
      'quality-alert-recipient-sql': 0, 'quality-id-source-sql': 0,
      'quality-attachment-source-select-sql': 0, 'quality-return-anchor-sql': 0,
      'quality-obligation-fact-arm-sql': 0, 'quality-lineage-terminal-sql': 0, 'quality-source-arm-head': 0,
    };
    for (const [name, count] of Object.entries(counts)) {
      expect(read(`adapters/${name}.ts`).match(/SELECT /g) ?? []).toHaveLength(count);
    }
    expect(inspectionEvidenceSource('x').startsWith('SELECT ')).toBe(true);
    expect(holdEvidenceSource('x').startsWith('SELECT ')).toBe(true);
    expect(inspectionMetricSource('x', '').startsWith('SELECT ')).toBe(true);
  });
});

describe('quality read-tail r26 retirement check', () => {
  test('retirement:ts-statements is strictly lower than the BEFORE=57 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(57);
    expect(total).toBe(1);
  });
});
