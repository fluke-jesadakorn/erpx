import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { qualityObligationFactArm } from '../../../packages/server-quality/src/adapters/quality-obligation-fact-arm-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-quality/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');

// Pre-dedupe obligation fact UNION arms copied byte-for-byte from the r22 source.
// Each arm differs only in projection, source relation and optional predicate.
const families = [
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

const supplyUnion = " SELECT 'hold' kind,hold.id,jsonb_build_array(to_jsonb(hold),\n ${qualityChildAggSql('inventory_quality_inspections','inspection','hold_id','hold.id')},\n ${qualityChildAggSql('inventory_quality_cancellations','event','hold_id','hold.id')},\n ${qualityChildAggSql('inventory_quality_disposition_events','event','hold_id','hold.id')}) fact,\n NOT COALESCE((hold.status='released' AND hold.last_result='pass' AND hold.released_at IS NOT NULL AND hold.released_by IS NOT NULL\n AND EXISTS(SELECT 1 FROM inventory_quality_inspections inspection WHERE inspection.hold_id=hold.id AND inspection.result='pass' AND inspection.inspected_by=hold.last_inspector)\n AND EXISTS(${qualityReceiptGuardSql('hold.id','quality.hold.release','hold.company_id','version','hold.version::text','hold.released_by')}))\n OR(hold.status='cancelled' AND EXISTS(SELECT 1 FROM inventory_quality_cancellations event JOIN command_receipts receipt\n ON event.id='quality-cancel-'||receipt.command_id AND receipt.record_id=hold.id AND receipt.actor_id=event.actor_id AND receipt.operation='quality.hold.cancel.approve'\n AND receipt.authority_json::jsonb->>'companyId'=hold.company_id AND receipt.result_json::jsonb->>'version'=hold.version::text\n WHERE event.hold_id=hold.id AND event.version=hold.version AND event.action='approve' AND event.actor_id=hold.cancelled_by))\n OR(hold.status='disposed' AND EXISTS(SELECT 1 FROM (${inventory.counts}) count JOIN inventory_quality_disposition_events event\n ON event.count_id=count.id AND event.hold_id=hold.id AND event.action='post'\n JOIN command_receipts receipt ON event.id='quality-disposition-'||receipt.command_id AND receipt.record_id=count.id AND receipt.operation='inventory.count.post'\n AND receipt.actor_id=event.actor_id AND receipt.authority_json::jsonb->>'companyId'=hold.company_id\n WHERE count.id=hold.disposition_count_id AND count.quality_hold_id=hold.id AND count.company_id=hold.company_id AND count.warehouse_id=hold.warehouse_id\n AND count.status='posted' AND count.approved_by=hold.disposed_by)),false) pending FROM holds hold\n UNION ALL SELECT 'alert',alert.id,to_jsonb(alert),hold.status NOT IN ('released','cancelled','disposed') AND\n (alert.acknowledged_by IS NULL OR alert.acknowledged_at IS NULL OR NOT EXISTS(${qualityReceiptGuardSql('alert.id','quality.alert.acknowledge','hold.company_id','version','alert.version::text','alert.acknowledged_by')}))\n FROM inventory_quality_alerts alert JOIN holds hold ON hold.id=alert.hold_id\n UNION ALL SELECT 'specification',spec.id,jsonb_build_array(to_jsonb(spec),${qualityChildAggSql('inventory_quality_spec_metrics','metric','spec_id','spec.id')}),\n NOT COALESCE((spec.status IN ('active','superseded') AND spec.approved_by IS NOT NULL AND spec.activated_at IS NOT NULL AND EXISTS(${qualityReceiptGuardSql('spec.id','quality.spec.activate','spec.company_id','status',\"'active'\",'spec.approved_by')}))\n OR(spec.status='cancelled' AND EXISTS(${qualityReceiptGuardSql('spec.id','quality.spec.cancel','spec.company_id','status',\"'cancelled'\")})),false)\n FROM inventory_quality_specs spec,scope WHERE spec.company_id=scope.company_id ORDER BY kind,id";
const manufacturingUnion = " SELECT 'quality-hold' kind,h.id,to_jsonb(h)::text fact FROM holds h WHERE h.status NOT IN ('released','cancelled','disposed')\n UNION ALL SELECT 'quality-alert',a.id,to_jsonb(a)::text FROM inventory_quality_alerts a JOIN holds h ON h.id=a.hold_id WHERE a.acknowledged_by IS NULL\n ORDER BY kind,id";

describe('quality obligation fact ARM dedupe (r23)', () => {
  test.each(families)('$name: helper reproduces the pre-dedupe arm byte-for-byte', ({ columns, source, predicate, before }) => {
    const sql = qualityObligationFactArm(columns, source, predicate);
    expect(sql).toBe(before);
    expect(sql.startsWith('SELECT ')).toBe(true);
    expect(sql).not.toContain('?');
    expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
  test('the five arms recompose both pre-dedupe obligation UNION blocks exactly', () => {
    const [a, b, c, d, e] = families.map(({ before }) => before);
    expect(` ${a}\n UNION ALL ${b}\n UNION ALL ${c}`).toBe(supplyUnion);
    expect(` ${d}\n UNION ALL ${e}\n ORDER BY kind,id`).toBe(manufacturingUnion);
  });
  test('both readers share the helper with no residual inline arms', () => {
    const supply = read('adapters/supply-package-obligations.ts');
    const manuf = read('adapters/manufacturing-package-obligations.ts');
    expect(supply.match(/qualityObligationFactArm\(/g)).toHaveLength(3);
    expect(manuf.match(/qualityObligationFactArm\(/g)).toHaveLength(2);
    for (const { before } of families) {
      expect(supply).not.toContain(before);
      expect(manuf).not.toContain(before);
    }
    expect(supply).toContain("import {qualityObligationFactArm} from './quality-obligation-fact-arm-sql'");
    expect(manuf).toContain("import {qualityObligationFactArm} from './quality-obligation-fact-arm-sql'");
    expect(read('adapters/quality-obligation-fact-arm-sql.ts').match(/SELECT /g) ?? []).toHaveLength(0);
    expect(read('adapters/quality-source-arm-head.ts').match(/SELECT /g)).toHaveLength(1);
  });
  test('the helper is read-only; locks, native and r1-r22 helpers stay untouched', () => {
    const helper = read('adapters/quality-obligation-fact-arm-sql.ts');
    expect(helper).not.toMatch(/\?|INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    expect(read('adapters/purchase-sources.ts')).toContain('qualityPurchaseSourceReadLocks');
    expect(read('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_specs');
    expect(read('adapters/sales-package-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_inspections,inventory_quality_disposition_events,inventory_quality_cancellations,inventory_quality_specs,inventory_quality_spec_metrics IN SHARE MODE');
    expect(read('adapters/attachments-policy-facts.ts')).toContain('descriptorTargets');
    expect(read('adapters/attachments-service-facts.ts')).toContain('UPDATE ${descriptor.table} SET version=version+1');
    expect(read('adapters/customer_return_hold_statement.rs')).toContain('customer_return_hold_statements');
    expect(read('adapters/customer_return_hold_http.rs')).toContain('customer_return_hold');
    const counts: Record<string, number> = {
      'quality-work-scope-sql': 1, 'native-source-wrapper-sql': 1, 'inspection-evidence-source-sql': 1,
      'quality-record-source-sql': 1, 'quality-lineage-recursion-sql': 2, 'quality-child-agg-sql': 1,
      'quality-receipt-guard-sql': 1, 'quality-source-rows-sql': 1, 'quality-scope-sql': 2,
      'quality-alert-recipient-sql': 1, 'quality-attachment-scope-read-sql': 1, 'quality-id-source-sql': 1,
      'quality-attachment-source-select-sql': 1, 'quality-source-union-sql': 1, 'quality-return-anchor-sql': 0,
      'quality-obligation-fact-arm-sql': 0, 'quality-source-arm-head': 1,
    };
    for (const [name, count] of Object.entries(counts)) {
      expect(read(`adapters/${name}.ts`).match(/SELECT /g) ?? []).toHaveLength(count);
    }
  });
});

describe('quality read-tail r23 retirement check', () => {
  test('retirement:ts-statements is strictly lower than the BEFORE=67 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(67);
    expect(total).toBe(33);
  });
});
