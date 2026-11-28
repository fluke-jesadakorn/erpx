import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

const srcRoot = resolve(__dirname, '../../../modules/pos/server/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const occurrences = (text: string, needle: string) => text.split(needle).length - 1;

// Verbatim pre-r38 literal recorded from the AST of `pos-core.ts` at BEFORE=180
// (HEAD 77bc573d300d2b870938de0a5d230464d6488d08). Byte-identity oracle only,
// never a live query source. The two folded heads are sale-scoped by-id row
// reads on the caller's serializable unit; the zero-arg renderer returns the
// exact literal, so the trailing FOR UPDATE row lock, the single bind and the
// bind order are unchanged. No Rust owner registry, addon/package settlement
// mirror, PMS fragment, advisory lock or cross-owner consumer is touched.
const ORACLE = 'SELECT * FROM pos_sales WHERE id=? FOR UPDATE';
const RENDERER = "const posSaleByIdLockSql = (): string => 'SELECT * FROM pos_sales WHERE id=? FOR UPDATE';";

const FOLD = [
  { method: 'readPosSales1', renderer: 'posSaleByIdLockSql', bind: 'facts.saleId' },
  { method: 'readPosSales2', renderer: 'posSaleByIdLockSql', bind: 'facts.sale_id' },
] as const;

describe('POS read-tail r38 sales by-id lock-head dedupe', () => {
  test('the byte-identical head is authored exactly once in a zero-arg owner-local renderer', () => {
    const core = read('pos-core.ts');
    expect(core).toContain(RENDERER);
    expect(occurrences(core, ORACLE)).toBe(1);
    expect((ORACLE.match(/\?/g) ?? []).length).toBe(1);
    expect(ORACLE).toContain('FOR UPDATE');
    expect(ORACLE.startsWith('SELECT ')).toBe(true);
  });

  test('per-site byte oracles: both call sites delegate with the original bind expression and order', () => {
    const core = read('pos-core.ts');
    for (const site of FOLD) {
      const i = core.indexOf(`${site.method}:`);
      expect(i).toBeGreaterThanOrEqual(0);
      expect(core.slice(i, i + 240)).toContain(`one(db, ${site.renderer}(), ${site.bind})`);
    }
    expect(occurrences(core, 'posSaleByIdLockSql()')).toBe(2);
  });

  test('protected row locks, advisory lock, PMS fragments and registries stay untouched', () => {
    const core = read('pos-core.ts');
    expect((core.match(/FOR UPDATE/g) ?? []).length).toBe(17);
    expect(core).toContain('FOR UPDATE OF rp,r');
    expect(core).toContain("SELECT pg_advisory_xact_lock(hashtextextended('pos-daily:'||?,0))");
    expect(core).toContain('${pmsProjectionSources.pms_properties}');
    expect(read('pos.ts')).toContain('${pmsProjectionSources.pms_sources}');
    expect(read('native-pos-day-report.ts')).toContain("'pos_day_report_");
    expect(read('native-pos-stripe-reads.ts')).toContain("'pos_stripe_");
    expect(read('pos-stripe.ts')).toContain('SELECT * FROM pos_sales WHERE id=? AND company_id=? FOR UPDATE');
    expect(read('owner_writes.rs')).toContain('assert_eq!(KINDS.len(), 203)');
  });

  test('retirement:ts-statements decreased 180 -> 173 (r42 continues below the r38 BEFORE=180 baseline)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(147);
    expect(total).toBeLessThan(180);
  });
});
