import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { qualityReturnAnchorSql } from '../../../packages/server-quality/src/adapters/quality-return-anchor-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-quality/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');

// Pre-dedupe return-anchor UNION arms copied byte-for-byte from HEAD (r21 source).
const rows = '${commerceQualityOriginRows}';
const origins = '${origins}';
const line = "${qualityIdInSourceSql(commerce.returnLines, 'line')}";
const lot = "${qualityIdInSourceSql(commerce.returnLots, 'lot')}";
const families = [
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

describe('quality return-anchor UNION arm dedupe (r21)', () => {
  test.each(families)('$name: helper reproduces the pre-dedupe arm byte-for-byte', ({ args, before }) => {
    const sql = qualityReturnAnchorSql(args[0], args[1], args[2]);
    expect(sql).toBe(before);
    expect(sql.match(/SELECT /g)).toHaveLength(1);
    expect(sql).not.toContain('?');
    expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
  test('the four arms recompose both pre-dedupe anchors CTEs exactly', () => {
    const [a, b, c, d] = families.map(({ args }) => qualityReturnAnchorSql(args[0], args[1], args[2]));
    expect(`WITH RECURSIVE anchors AS(\n ${a}\n UNION ${b}),`).toBe(
      `WITH RECURSIVE anchors AS(\n ${families[0].before}\n UNION ${families[1].before}),`);
    expect(`WITH RECURSIVE anchors AS(\n ${c}\n UNION ${d}),`).toBe(
      `WITH RECURSIVE anchors AS(\n ${families[2].before}\n UNION ${families[3].before}),`);
  });
  test('both readers share the helper with no residual inline arms', () => {
    const sales = read('adapters/sales-package-sources.ts');
    const ecommerce = read('adapters/ecommerce-package-sources.ts');
    expect(sales.match(/qualityReturnAnchorSql\(/g)).toHaveLength(2);
    expect(ecommerce.match(/qualityReturnAnchorSql\(/g)).toHaveLength(2);
    for (const { before } of families) {
      expect(sales).not.toContain(before);
      expect(ecommerce).not.toContain(before);
    }
    expect(sales).toContain("import {qualityReturnAnchorSql} from './quality-return-anchor-sql'");
    expect(ecommerce).toContain("import {qualityReturnAnchorSql} from './quality-return-anchor-sql'");
    expect(read('adapters/quality-return-anchor-sql.ts').match(/SELECT /g) ?? []).toHaveLength(0);
    expect(read('adapters/quality-source-arm-head.ts').match(/SELECT /g)).toHaveLength(1);
  });
  test('the helper is read-only; locks, native and r1-r20 helpers stay untouched', () => {
    const helper = read('adapters/quality-return-anchor-sql.ts');
    expect(helper).not.toMatch(/\?|INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
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
      'quality-receipt-guard-sql': 1, 'quality-source-rows-sql': 1, 'quality-scope-sql': 1,
      'quality-alert-recipient-sql': 0, 'quality-attachment-scope-read-sql': 1, 'quality-id-source-sql': 1,
      'quality-attachment-source-select-sql': 1, 'quality-return-anchor-sql': 0,
      'quality-obligation-fact-arm-sql': 0, 'quality-source-arm-head': 1,
    };
    for (const [name, count] of Object.entries(counts)) {
      expect(read(`adapters/${name}.ts`).match(/SELECT /g) ?? []).toHaveLength(count);
    }
  });
});

describe('quality read-tail r21 retirement check', () => {
  test('retirement:ts-statements is strictly lower than the BEFORE=70 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(70);
    expect(total).toBe(26);
  });
});
