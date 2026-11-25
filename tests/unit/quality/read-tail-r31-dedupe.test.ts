import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { qualityPackageScopeCte, qualityClockCte } from '../../../packages/server-quality/src/adapters/quality-scope-sql';
import { packageClockSql } from '@erpx/server-commercial/public/purchased-capability-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-quality/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');

// Pre-dedupe byte oracles captured from the r30 source. The two package-scope
// CTE heads were inline `(SELECT ...)` literals; both now route through one
// owner-local scopeSql head. Every rendered string below is unchanged.
const before = {
  packageScope: '(SELECT ?::text company_id,?::boolean full)',
  clock: (name: string) => `WITH ${name} AS MATERIALIZED (SELECT ${packageClockSql} now_ms)`,
  scopeTuple: {
    current: '(SELECT ?::text company_id,?::text branch_id,NULL::text department_id) quality_scope',
    actor: '(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope',
    calendar: '(SELECT h.company_id,h.branch_id,NULL::text department_id) quality_calendar_scope',
  },
};

const kpi = () => {
  let total = 0;
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name.endsWith('.ts')) {
        total += (readFileSync(path, 'utf8').match(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g) ?? []).length;
      }
    }
  };
  walk(srcRoot);
  return total;
};

describe('quality read-tail r31 package-scope head dedupe', () => {
  test('the shared scopeSql head reproduces both pre-dedupe CTE strings byte-for-byte', () => {
    expect(qualityPackageScopeCte).toBe(before.packageScope);
    expect(qualityPackageScopeCte).not.toContain('${');
    for (const name of ['quality_source_clock', 'quality_entry_clock', 'quality_calendar_clock']) {
      expect(qualityClockCte(name)).toBe(before.clock(name));
    }
  });

  test('binds and guard order are unchanged by the head fold', () => {
    // The Company/full CTE keeps exactly its two binds, in order; the clock
    // CTE keeps none. No bind is introduced or reordered by scopeSql.
    expect(qualityPackageScopeCte.match(/\?/g)).toHaveLength(2);
    expect(qualityPackageScopeCte).toContain('?::text company_id,?::boolean full');
    expect(qualityClockCte('x').match(/\?/g)).toBeNull();
    expect(qualityClockCte('x')).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    expect(qualityPackageScopeCte).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('one owner-local head helper replaces the two inline scope heads', () => {
    const head = read('adapters/quality-scope-sql.ts');
    expect(head.match(/SELECT /g)).toHaveLength(1);
    expect(head).toContain('const scopeSql = (columns:string) => `(SELECT ${columns})`');
    expect(head).toContain('qualityPackageScopeCte = scopeSql("?::text company_id,?::boolean full")');
    expect(head).toContain('${scopeSql(`${packageClockSql} now_ms`)}');
    expect(head).not.toContain('qualityPackageScopeCte = "(SELECT');
    expect(head).not.toContain('AS MATERIALIZED (SELECT ${packageClockSql}');
  });

  test('call sites keep the same CTE, scope-tuple and guard compositions', () => {
    for (const path of ['adapters/supply-package-obligations.ts', 'adapters/manufacturing-package-obligations.ts']) {
      expect(read(path)).toContain('WITH scope AS${qualityPackageScopeCte}');
      expect(read(path)).not.toContain('WITH scope AS(SELECT');
    }
    // Guard order in the Supply package obligation reader is unchanged.
    const supply = read('adapters/supply-package-obligations.ts');
    const pass = supply.indexOf('EXISTS(SELECT 1 FROM inventory_quality_inspections inspection');
    const cancel = supply.indexOf('EXISTS(SELECT 1 FROM inventory_quality_cancellations event');
    const dispose = supply.indexOf('EXISTS(SELECT 1 FROM (${inventory.counts}) count');
    expect(pass).toBeGreaterThan(-1);
    expect(cancel).toBeGreaterThan(pass);
    expect(dispose).toBeGreaterThan(cancel);

    // The scope-tuple heads carried by purchases/attachment-read/calendar are
    // unaffected; they remain the exact strings pinned before this pass.
    const manufacturing = read('adapters/manufacturing-package-obligations.ts');
    expect(manufacturing).toContain('orders AS MATERIALIZED(SELECT o.*');
    expect(manufacturing).toContain('holds AS MATERIALIZED(SELECT h.*');
    expect(read('adapters/purchases.ts')).toContain('qualityWorkScopeTuple("?::text company_id,?::text branch_id,NULL::text department_id","quality_scope")');
    expect(read('adapters/attachment-read.ts')).toContain('qualityWorkScopeTuple("?::text company_id,?::text branch_id,?::text department_id","workforce_actor_scope","")');
    expect(read('adapters/calendar-quality-facts.ts')).toContain('qualityWorkScopeTuple("h.company_id,h.branch_id,NULL::text department_id","quality_calendar_scope")');
    expect(read('adapters/quality-work-scope-sql.ts')).toContain('`(SELECT ${columns})${separator}${alias}`');

    const clockSites = [
      ['adapters/record-purchases.ts', 'quality_source_clock'],
      ['adapters/product-entry.ts', 'quality_entry_clock'],
      ['adapters/calendar-quality-facts.ts', 'quality_calendar_clock'],
    ] as const;
    for (const [path, name] of clockSites) {
      expect(read(path)).toContain(`qualityClockCte("${name}")`);
      expect(read(path)).not.toContain(`WITH ${name} AS MATERIALIZED (SELECT`);
    }
  });

  test('retirement:ts-statements is strictly lower than the BEFORE=31 baseline', () => {
    expect(kpi()).toBeLessThan(31);
    expect(kpi()).toBe(25);
  });
});
