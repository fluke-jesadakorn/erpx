import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { qualityWorkScopeTuple } from '../../../packages/server-quality/src/adapters/quality-work-scope-sql';
import { nativeSourceWhere } from '../../../packages/server-quality/src/adapters/native-source-wrapper-sql';
import { inspectionEvidenceSource } from '../../../packages/server-quality/src/adapters/inspection-evidence-source-sql';
import { commercialOwnerProjectionSources as commerce } from '@erpx/server-commerce/public/commercial-owner-projection-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-quality/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');

describe('quality work-scope tuple family dedupe (r16)', () => {
  const inline = [
    ['?::text company_id,?::text branch_id,NULL::text department_id', 'quality_scope', ' ',
      '(SELECT ?::text company_id,?::text branch_id,NULL::text department_id) quality_scope'],
    ['h.company_id,h.branch_id,NULL::text department_id', 'quality_calendar_scope', ' ',
      '(SELECT h.company_id,h.branch_id,NULL::text department_id) quality_calendar_scope'],
    ['?::text company_id,?::text branch_id,?::text department_id', 'workforce_actor_scope', '',
      '(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope'],
  ] as const;
  test.each(inline)('tuple %s reproduces the inline derived table byte-for-byte', (columns, alias, separator, before) => {
    expect(qualityWorkScopeTuple(columns, alias, separator)).toBe(before);
    expect(qualityWorkScopeTuple(columns, alias, separator).match(/SELECT /g)).toHaveLength(1);
  });
  test('the three current-scope readers share the tuple with no inline copies', () => {
    for (const path of ['adapters/purchases.ts', 'adapters/calendar-quality-facts.ts', 'adapters/attachment-read.ts']) {
      expect(read(path)).toContain('qualityWorkScopeTuple(');
    }
    expect(read('adapters/purchases.ts')).not.toContain('(SELECT ?::text company_id,?::text branch_id,NULL::text department_id) quality_scope');
    expect(read('adapters/calendar-quality-facts.ts')).not.toContain('(SELECT h.company_id,h.branch_id,NULL::text department_id)');
    expect(read('adapters/attachment-read.ts')).not.toContain('FROM(SELECT ?::text company_id');
    expect(read('adapters/quality-work-scope-sql.ts').match(/SELECT /g)).toHaveLength(1);
  });
});

describe('quality native source wrapper family dedupe (r16)', () => {
  test('one parameterized wrapper reproduces both settlement source SELECTs', () => {
    expect(nativeSourceWhere('RET_ORIGINS', "native.kind='customer-return' AND native.owner_id IN (SELECT id FROM (RETURNS) returned)"))
      .toBe("SELECT native.* FROM (RET_ORIGINS) native WHERE native.kind='customer-return' AND native.owner_id IN (SELECT id FROM (RETURNS) returned)");
    expect(nativeSourceWhere('RET_HOLDS', 'native.id IN (HOLD_IDS)'))
      .toBe('SELECT native.* FROM (RET_HOLDS) native WHERE native.id IN (HOLD_IDS)');
  });
  test('ecommerce settlement sources call the wrapper with no inline native projection', () => {
    const text = read('adapters/ecommerce-package-sources.ts');
    expect(text.match(/nativeSourceWhere\(/g)).toHaveLength(2);
    expect(text).not.toContain('SELECT native.* FROM (');
    expect(read('adapters/native-source-wrapper-sql.ts').match(/SELECT /g)).toHaveLength(1);
  });
});

describe('quality executive evidence join family dedupe (r16)', () => {
  const join = `\n    FROM inventory_quality_inspections i
    JOIN inventory_quality_holds h ON h.id=i.hold_id
    JOIN (${commerce.commerce_products}) p ON p.id=h.product_id AND p.company_id=h.company_id
    JOIN (${commerce.commerce_warehouses}) w ON w.id=h.warehouse_id AND w.company_id=h.company_id`;
  const list = 'i.id,i.hold_id,i.result,i.created_at,i.reviewed_version,p.name product_name,w.name warehouse_name';
  const detail = 'i.id,i.hold_id,i.result,i.evidence,i.inspected_by,i.created_at,i.reviewed_version,\n      p.name product_name,w.name warehouse_name';
  test('one parameterized projection reproduces both inspection evidence SELECTs byte-for-byte', () => {
    expect(inspectionEvidenceSource(list)).toBe(`SELECT ${list}${join}`);
    expect(inspectionEvidenceSource(detail)).toBe(`SELECT ${detail}${join}`);
    for (const projection of [list, detail]) expect(inspectionEvidenceSource(projection)).toMatch(/^SELECT /);
  });
  test('the executive evidence readers share the join with no inline copies', () => {
    const text = read('adapters/executive-main-evidence-sources.ts');
    expect(text.match(/inspectionEvidenceSource\(/g)).toHaveLength(2);
    expect(text).not.toContain(`SELECT ${list}`);
    expect(text).not.toContain('SELECT i.id,i.hold_id,i.result,i.evidence,i.inspected_by');
    expect(read('adapters/inspection-evidence-source-sql.ts').match(/SELECT /g)).toHaveLength(1);
  });
});

describe('quality read-tail r16 retirement check', () => {
  test('retirement:ts-statements stays strictly below the BEFORE=75 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(75);
    expect(total).toBe(33);
  });
  test('the three helpers are read-only and leave locks and prior families untouched', () => {
    for (const name of ['quality-work-scope-sql', 'native-source-wrapper-sql', 'inspection-evidence-source-sql']) {
      expect(read(`adapters/${name}.ts`)).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    expect(read('adapters/sales-package-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_inspections,inventory_quality_disposition_events,inventory_quality_cancellations');
    expect(read('adapters/attachment-read.ts')).toContain('qualityAttachmentReadSourceLocks');
    for (const name of ['quality-child-agg-sql', 'quality-receipt-guard-sql', 'quality-source-rows-sql', 'quality-record-source-sql', 'quality-lineage-recursion-sql']) {
      expect(read(`adapters/${name}.ts`).match(/SELECT /g)).toHaveLength(name === 'quality-lineage-recursion-sql' ? 2 : 1);
    }
  });
});
