import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { qualityReturnAnchorSql } from '../../../packages/server-quality/src/adapters/quality-return-anchor-sql';
import { qualityObligationFactArm } from '../../../packages/server-quality/src/adapters/quality-obligation-fact-arm-sql';
import { qualitySourceArmHead } from '../../../packages/server-quality/src/adapters/quality-source-arm-head';

const srcRoot = resolve(__dirname, '../../../packages/server-quality/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');

// Byte oracles copied from the r22 (four return anchors) and r23 (five
// obligation fact arms) tests. Neither family's rendered SQL changed.
const rows = '${commerceQualityOriginRows}';
const origins = '${origins}';
const line = "${qualityIdInSourceSql(commerce.returnLines, 'line')}";
const lot = "${qualityIdInSourceSql(commerce.returnLots, 'lot')}";
const anchorFamilies = [
  {
    name: 'sales customer_return hold anchor',
    args: ['h.id,h.company_id source_company', 'inventory_quality_holds h', " WHERE h.source_type='customer_return'"],
    before: `SELECT h.id,h.company_id source_company FROM inventory_quality_holds h WHERE h.source_type='customer_return'`,
  },
  {
    name: 'sales commerce origin anchor',
    args: ['origin.hold_id,origin.company_id', `(${rows}) origin`, " WHERE origin.kind='customer-return'"],
    before: `SELECT origin.hold_id,origin.company_id FROM (${rows}) origin WHERE origin.kind='customer-return'`,
  },
  {
    name: 'ecommerce native origin anchor',
    args: ['origin.hold_id id', `(${origins}) origin`],
    before: `SELECT origin.hold_id id FROM (${origins}) origin`,
  },
  {
    name: 'ecommerce customer_return hold anchor',
    args: ['hold.id', 'inventory_quality_holds hold',
      ` WHERE hold.source_type='customer_return' AND (hold.source_id ${line} OR hold.source_id ${lot})`],
    before: `SELECT hold.id FROM inventory_quality_holds hold WHERE hold.source_type='customer_return' AND (hold.source_id ${line} OR hold.source_id ${lot})`,
  },
] as const;

const armFamilies = [
  {
    name: "supply hold fact arm",
    columns: "'hold' kind,hold.id,jsonb_build_array(to_jsonb(hold),\n ${qualityChildAggSql('inventory_quality_inspections','inspection','hold_id','hold.id')},\n ${qualityChildAggSql('inventory_quality_cancellations','event','hold_id','hold.id')},\n ${qualityChildAggSql('inventory_quality_disposition_events','event','hold_id','hold.id')}) fact,\n NOT COALESCE((hold.status='released' AND hold.last_result='pass' AND hold.released_at IS NOT NULL AND hold.released_by IS NOT NULL\n AND EXISTS(SELECT 1 FROM inventory_quality_inspections inspection WHERE inspection.hold_id=hold.id AND inspection.result='pass' AND inspection.inspected_by=hold.last_inspector)\n AND EXISTS(${qualityReceiptGuardSql('hold.id','quality.hold.release','hold.company_id','version','hold.version::text','hold.released_by')}))\n OR(hold.status='cancelled' AND EXISTS(SELECT 1 FROM inventory_quality_cancellations event JOIN command_receipts receipt\n ON event.id='quality-cancel-'||receipt.command_id AND receipt.record_id=hold.id AND receipt.actor_id=event.actor_id AND receipt.operation='quality.hold.cancel.approve'\n AND receipt.authority_json::jsonb->>'companyId'=hold.company_id AND receipt.result_json::jsonb->>'version'=hold.version::text\n WHERE event.hold_id=hold.id AND event.version=hold.version AND event.action='approve' AND event.actor_id=hold.cancelled_by))\n OR(hold.status='disposed' AND EXISTS(SELECT 1 FROM (${inventory.counts}) count JOIN inventory_quality_disposition_events event\n ON event.count_id=count.id AND event.hold_id=hold.id AND event.action='post'\n JOIN command_receipts receipt ON event.id='quality-disposition-'||receipt.command_id AND receipt.record_id=count.id AND receipt.operation='inventory.count.post'\n AND receipt.actor_id=event.actor_id AND receipt.authority_json::jsonb->>'companyId'=hold.company_id\n WHERE count.id=hold.disposition_count_id AND count.quality_hold_id=hold.id AND count.company_id=hold.company_id AND count.warehouse_id=hold.warehouse_id\n AND count.status='posted' AND count.approved_by=hold.disposed_by)),false) pending",
    source: "holds hold",
    predicate: "",
    before: "SELECT 'hold' kind,hold.id,jsonb_build_array(to_jsonb(hold),\n ${qualityChildAggSql('inventory_quality_inspections','inspection','hold_id','hold.id')},\n ${qualityChildAggSql('inventory_quality_cancellations','event','hold_id','hold.id')},\n ${qualityChildAggSql('inventory_quality_disposition_events','event','hold_id','hold.id')}) fact,\n NOT COALESCE((hold.status='released' AND hold.last_result='pass' AND hold.released_at IS NOT NULL AND hold.released_by IS NOT NULL\n AND EXISTS(SELECT 1 FROM inventory_quality_inspections inspection WHERE inspection.hold_id=hold.id AND inspection.result='pass' AND inspection.inspected_by=hold.last_inspector)\n AND EXISTS(${qualityReceiptGuardSql('hold.id','quality.hold.release','hold.company_id','version','hold.version::text','hold.released_by')}))\n OR(hold.status='cancelled' AND EXISTS(SELECT 1 FROM inventory_quality_cancellations event JOIN command_receipts receipt\n ON event.id='quality-cancel-'||receipt.command_id AND receipt.record_id=hold.id AND receipt.actor_id=event.actor_id AND receipt.operation='quality.hold.cancel.approve'\n AND receipt.authority_json::jsonb->>'companyId'=hold.company_id AND receipt.result_json::jsonb->>'version'=hold.version::text\n WHERE event.hold_id=hold.id AND event.version=hold.version AND event.action='approve' AND event.actor_id=hold.cancelled_by))\n OR(hold.status='disposed' AND EXISTS(SELECT 1 FROM (${inventory.counts}) count JOIN inventory_quality_disposition_events event\n ON event.count_id=count.id AND event.hold_id=hold.id AND event.action='post'\n JOIN command_receipts receipt ON event.id='quality-disposition-'||receipt.command_id AND receipt.record_id=count.id AND receipt.operation='inventory.count.post'\n AND receipt.actor_id=event.actor_id AND receipt.authority_json::jsonb->>'companyId'=hold.company_id\n WHERE count.id=hold.disposition_count_id AND count.quality_hold_id=hold.id AND count.company_id=hold.company_id AND count.warehouse_id=hold.warehouse_id\n AND count.status='posted' AND count.approved_by=hold.disposed_by)),false) pending FROM holds hold",
  },
  {
    name: "supply alert fact arm",
    columns: "'alert',alert.id,to_jsonb(alert),hold.status NOT IN ('released','cancelled','disposed') AND\n (alert.acknowledged_by IS NULL OR alert.acknowledged_at IS NULL OR NOT EXISTS(${qualityReceiptGuardSql('alert.id','quality.alert.acknowledge','hold.company_id','version','alert.version::text','alert.acknowledged_by')}))\n",
    source: "inventory_quality_alerts alert JOIN holds hold ON hold.id=alert.hold_id",
    predicate: "",
    before: "SELECT 'alert',alert.id,to_jsonb(alert),hold.status NOT IN ('released','cancelled','disposed') AND\n (alert.acknowledged_by IS NULL OR alert.acknowledged_at IS NULL OR NOT EXISTS(${qualityReceiptGuardSql('alert.id','quality.alert.acknowledge','hold.company_id','version','alert.version::text','alert.acknowledged_by')}))\n FROM inventory_quality_alerts alert JOIN holds hold ON hold.id=alert.hold_id",
  },
  {
    name: "supply specification fact arm",
    columns: "'specification',spec.id,jsonb_build_array(to_jsonb(spec),${qualityChildAggSql('inventory_quality_spec_metrics','metric','spec_id','spec.id')}),\n NOT COALESCE((spec.status IN ('active','superseded') AND spec.approved_by IS NOT NULL AND spec.activated_at IS NOT NULL AND EXISTS(${qualityReceiptGuardSql('spec.id','quality.spec.activate','spec.company_id','status',\"'active'\",'spec.approved_by')}))\n OR(spec.status='cancelled' AND EXISTS(${qualityReceiptGuardSql('spec.id','quality.spec.cancel','spec.company_id','status',\"'cancelled'\")})),false)\n",
    source: "inventory_quality_specs spec,scope",
    predicate: " WHERE spec.company_id=scope.company_id ORDER BY kind,id",
    before: "SELECT 'specification',spec.id,jsonb_build_array(to_jsonb(spec),${qualityChildAggSql('inventory_quality_spec_metrics','metric','spec_id','spec.id')}),\n NOT COALESCE((spec.status IN ('active','superseded') AND spec.approved_by IS NOT NULL AND spec.activated_at IS NOT NULL AND EXISTS(${qualityReceiptGuardSql('spec.id','quality.spec.activate','spec.company_id','status',\"'active'\",'spec.approved_by')}))\n OR(spec.status='cancelled' AND EXISTS(${qualityReceiptGuardSql('spec.id','quality.spec.cancel','spec.company_id','status',\"'cancelled'\")})),false)\n FROM inventory_quality_specs spec,scope WHERE spec.company_id=scope.company_id ORDER BY kind,id",
  },
  {
    name: "manufacturing hold fact arm",
    columns: "'quality-hold' kind,h.id,to_jsonb(h)::text fact",
    source: "holds h",
    predicate: " WHERE h.status NOT IN ('released','cancelled','disposed')",
    before: "SELECT 'quality-hold' kind,h.id,to_jsonb(h)::text fact FROM holds h WHERE h.status NOT IN ('released','cancelled','disposed')",
  },
  {
    name: "manufacturing alert fact arm",
    columns: "'quality-alert',a.id,to_jsonb(a)::text",
    source: "inventory_quality_alerts a JOIN holds h ON h.id=a.hold_id",
    predicate: " WHERE a.acknowledged_by IS NULL",
    before: "SELECT 'quality-alert',a.id,to_jsonb(a)::text FROM inventory_quality_alerts a JOIN holds h ON h.id=a.hold_id WHERE a.acknowledged_by IS NULL",
  },
] as const;

describe('quality hold-source pair head dedupe (r25)', () => {
  test.each(anchorFamilies)('return anchor $name: both helpers reproduce pre-dedupe bytes', ({ args, before }) => {
    const sql = qualityReturnAnchorSql(args[0], args[1], args[2]);
    expect(sql).toBe(before);
    expect(qualityObligationFactArm(args[0], args[1], args[2])).toBe(before);
    expect(sql.match(/SELECT /g)).toHaveLength(1);
    expect(sql).not.toContain('?');
    expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
  test.each(armFamilies)('obligation arm $name: both helpers reproduce pre-dedupe bytes', ({ columns, source, predicate, before }) => {
    const sql = qualityObligationFactArm(columns, source, predicate);
    expect(sql).toBe(before);
    expect(qualityReturnAnchorSql(columns, source, predicate)).toBe(before);
    expect(sql.match(/SELECT /g)?.length ?? 0).toBeGreaterThanOrEqual(1);
    expect(sql.startsWith('SELECT ')).toBe(true);
    expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
  test('one shared head emits the identical parameterized template', () => {
    expect(qualitySourceArmHead('a', 'b')).toBe('SELECT a FROM b');
    expect(qualitySourceArmHead('a', 'b', ' WHERE c')).toBe('SELECT a FROM b WHERE c');
    const head = read('adapters/quality-source-arm-head.ts');
    expect(head).toContain('=> `SELECT ${columns} FROM ${source}${predicate}`');
    expect(head.match(/SELECT /g)).toHaveLength(1);
    expect(head).not.toMatch(/\?|INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
  test('both prior helpers delegate to the shared head with no residual inline head', () => {
    for (const name of ['quality-return-anchor-sql', 'quality-obligation-fact-arm-sql']) {
      const helper = read(`adapters/${name}.ts`);
      expect(helper).toContain("import { qualitySourceArmHead } from './quality-source-arm-head';");
      expect(helper.match(/SELECT /g) ?? []).toHaveLength(0);
      expect(helper).not.toMatch(/\?|INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    expect(read('adapters/quality-return-anchor-sql.ts')).toContain(') => qualitySourceArmHead(columns, source, predicate);');
    expect(read('adapters/quality-obligation-fact-arm-sql.ts')).toContain(') => qualitySourceArmHead(columns, source, predicate);');
  });
  test('all nine call sites keep their helper, import and binds', () => {
    const sales = read('adapters/sales-package-sources.ts');
    const ecommerce = read('adapters/ecommerce-package-sources.ts');
    const supply = read('adapters/supply-package-obligations.ts');
    const manuf = read('adapters/manufacturing-package-obligations.ts');
    expect(sales.match(/qualityReturnAnchorSql\(/g)).toHaveLength(2);
    expect(ecommerce.match(/qualityReturnAnchorSql\(/g)).toHaveLength(2);
    expect(supply.match(/qualityObligationFactArm\(/g)).toHaveLength(3);
    expect(manuf.match(/qualityObligationFactArm\(/g)).toHaveLength(2);
    expect(sales).toContain("import {qualityReturnAnchorSql} from './quality-return-anchor-sql';");
    expect(ecommerce).toContain("import {qualityReturnAnchorSql} from './quality-return-anchor-sql';");
    expect(supply).toContain("import {qualityObligationFactArm} from './quality-obligation-fact-arm-sql';");
    expect(manuf).toContain("import {qualityObligationFactArm} from './quality-obligation-fact-arm-sql';");
    for (const { before } of [...anchorFamilies, ...armFamilies]) {
      for (const text of [sales, ecommerce, supply, manuf]) expect(text).not.toContain(before);
    }
    expect(supply).toContain('.bind(input.companyId,input.packageId==="supply")');
    expect(manuf).toContain('.bind(input.companyId,input.packageId==="manufacturing")');
  });
  test('locks, native and r1-r23 helpers stay untouched', () => {
    expect(read('adapters/purchase-sources.ts')).toContain('qualityPurchaseSourceReadLocks');
    expect(read('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_specs');
    expect(read('adapters/sales-package-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_inspections,inventory_quality_disposition_events,inventory_quality_cancellations,inventory_quality_specs,inventory_quality_spec_metrics IN SHARE MODE');
    expect(read('adapters/attachments-policy-facts.ts')).toContain('descriptorTargets');
    expect(read('adapters/attachments-service-facts.ts')).toContain('UPDATE ${descriptor.table} SET version=version+1');
    expect(read('adapters/customer_return_hold_statement.rs')).toContain('customer_return_hold_statements');
    expect(read('adapters/customer_return_hold_http.rs')).toContain('customer_return_hold');
    const counts: Record<string, number> = {
      'quality-work-scope-sql': 1, 'native-source-wrapper-sql': 0, 'inspection-evidence-source-sql': 1,
      'quality-lineage-recursion-sql': 1, 'quality-child-agg-sql': 1,
      'quality-receipt-guard-sql': 1, 'quality-source-rows-sql': 0, 'quality-scope-sql': 1,
      'quality-alert-recipient-sql': 0, 'quality-attachment-scope-read-sql': 0, 'quality-id-source-sql': 1,
      'quality-attachment-source-select-sql': 0, 'quality-return-anchor-sql': 0,
      'quality-obligation-fact-arm-sql': 0, 'quality-lineage-terminal-sql': 0, 'quality-source-arm-head': 1,
    };
    for (const [name, count] of Object.entries(counts)) {
      expect(read(`adapters/${name}.ts`).match(/SELECT /g) ?? []).toHaveLength(count);
    }
  });
});

describe('quality read-tail r25 retirement check', () => {
  test('retirement:ts-statements is strictly lower than the BEFORE=59 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(59);
    expect(total).toBe(22);
  });
});
