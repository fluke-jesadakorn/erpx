import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

const srcRoot = resolve(__dirname, '../../../packages/server-pos/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const occurrences = (text: string, needle: string) => text.split(needle).length - 1;

// Verbatim pre-r37 literals recorded from the AST of `pos-core.ts` at BEFORE=182
// (git blob 3680103d6e7c81991f66ab383b3bbc578f98f4aa). These are byte-identity
// oracles only, never live query sources. The two folded heads are each a
// Company/sale-scoped by-id row read on the caller's serializable unit; the
// renderer returns the exact literal, so the trailing FOR UPDATE row lock, the
// single bind and the bind order are unchanged. r38 folds the `pos_sales` lock pair
// into one zero-arg renderer; ORACLE.sales remains a byte oracle only.
const ORACLE = {
  fulfillment: 'SELECT * FROM pos_fulfillments WHERE id=? FOR UPDATE',
  register: 'SELECT * FROM pos_registers WHERE id=? FOR UPDATE',
  sales: 'SELECT * FROM pos_sales WHERE id=? FOR UPDATE',
} as const;

const FOLD = [
  { method: 'readPosFulfillments1', renderer: 'posFulfillmentByIdLockSql', bind: 'facts.itemId' },
  { method: 'readPosFulfillments5', renderer: 'posFulfillmentByIdLockSql', bind: 'facts.id' },
  { method: 'readPosRegisters3', renderer: 'posRegisterByIdLockSql', bind: 'facts.id' },
  { method: 'readPosRegisters4', renderer: 'posRegisterByIdLockSql', bind: 'facts.registerId' },
] as const;

const RENDERERS = {
  fulfillment: `const posFulfillmentByIdLockSql = (): string => '${ORACLE.fulfillment}';`,
  register: `const posRegisterByIdLockSql = (): string => '${ORACLE.register}';`,
} as const;

describe('POS read-tail r37 owner-local lock-head dedupe', () => {
  test('each byte-identical head is authored exactly once in an owner-local renderer', () => {
    const core = read('pos-core.ts');
    expect(core).toContain(RENDERERS.fulfillment);
    expect(core).toContain(RENDERERS.register);
    expect(occurrences(core, ORACLE.fulfillment)).toBe(1);
    expect(occurrences(core, ORACLE.register)).toBe(1);
    for (const sql of [ORACLE.fulfillment, ORACLE.register]) {
      expect((sql.match(/\?/g) ?? []).length).toBe(1);
      expect(sql).toContain('FOR UPDATE');
      expect(sql.startsWith('SELECT ')).toBe(true);
    }
  });

  test('per-site byte oracles: all four call sites delegate with the original bind expression and order', () => {
    const core = read('pos-core.ts');
    for (const site of FOLD) {
      const i = core.indexOf(`${site.method}:`);
      expect(i).toBeGreaterThanOrEqual(0);
      expect(core.slice(i, i + 240)).toContain(`one(db, ${site.renderer}(), ${site.bind})`);
    }
    expect(occurrences(core, 'posFulfillmentByIdLockSql()')).toBe(2);
    expect(occurrences(core, 'posRegisterByIdLockSql()')).toBe(2);
  });

  test('r38 sales lock fold, advisory lock, PMS fragments and registries stay pinned', () => {
    const core = read('pos-core.ts');
    // r38 folds both sales by-id lock sites into one zero-arg owner-local renderer.
    expect(occurrences(core, ORACLE.sales)).toBe(1);
    expect(core).toContain("const posSaleByIdLockSql = (): string => 'SELECT * FROM pos_sales WHERE id=? FOR UPDATE';");
    expect(core).toContain('one(db, posSaleByIdLockSql(), facts.saleId)');
    expect(core).toContain('one(db, posSaleByIdLockSql(), facts.sale_id)');
    expect((core.match(/FOR UPDATE/g) ?? []).length).toBe(17);
    expect(core).toContain('FOR UPDATE OF rp,r');
    expect(core).toContain("SELECT pg_advisory_xact_lock(hashtextextended('pos-daily:'||?,0))");
    expect(core).toContain('${pmsProjectionSources.pms_properties}');
    expect(read('pos.ts')).toContain('${pmsProjectionSources.pms_sources}');
    expect(read('native-pos-day-report.ts')).toContain("'pos_day_report_");
    expect(read('native-pos-stripe-reads.ts')).toContain("'pos_stripe_");
    expect(read('owner_writes.rs')).toContain('assert_eq!(KINDS.len(), 196)');
  });

  test('retirement:ts-statements decreased 182 -> 176 (r39 continues below the r37 BEFORE=182 baseline)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(176);
    expect(total).toBeLessThan(182);
  });
});
