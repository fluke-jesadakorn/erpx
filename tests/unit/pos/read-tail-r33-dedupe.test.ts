import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

const srcRoot = resolve(__dirname, '../../../packages/server-pos/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');

// Verbatim pre-r33 literal recorded from the AST of `pos-core.ts` at BEFORE=189
// (source SHA 0a05fb451bda64fccb1b107fdbd228ce7d31f87c). This is a byte-identity
// oracle only, never a live query source. `readPosPayments13` is a Company-scoped
// by-id row read on the caller's serializable unit; the retired TS literal and the
// reused native owner registry statement carry the same trailing `FOR UPDATE`, so
// the row-lock semantics are unchanged by this cutover.
const SQL = 'SELECT * FROM pos_payments WHERE id=? FOR UPDATE';
const KIND = 'stripe_readpospaymentsbyid_527';
const BINDS = 1;

describe('POS read-tail r33 by-id payment native cutover', () => {
  test('native bridge reuses the existing by-id payment kind without a new registry entry', () => {
    const bridge = read('native-pos-core-statement.ts');
    expect(bridge).toContain(`'${KIND}'`);
    expect(bridge).toContain('nativePosPaymentByIdStatement');
    // No new by-id payment kind was added by r33; r35 mounted four Stripe
    // keyed reads and r36 mounted two day-planner sale-child reads, so the
    // registry length now pins the r36 value of 196.
    expect(read('owner_writes.rs')).toContain('assert_eq!(KINDS.len(), 196)');
  });

  test('owner registry already holds the retired literal byte-identically with its FOR UPDATE lock', () => {
    const owner = read('owner_writes.rs');
    expect(owner).toContain(`STRIPE_READPOSPAYMENTSBYID_527: &str = "${KIND}"`);
    expect(owner).toContain(`"${SQL}"`);
    expect((SQL.match(/\?/g) ?? []).length).toBe(BINDS);
    expect(SQL).toContain('FOR UPDATE');
    expect(owner).toContain('STRIPE_READPOSPAYMENTSBYID_527 =>');
  });

  test('former inline literal is gone and the call site delegates to the native bridge', () => {
    const core = read('pos-core.ts');
    expect(core).not.toContain(SQL);
    expect(core).toContain('nativePosPaymentByIdStatement');
    expect(core.match(/nativePosPaymentByIdStatement<PosRow>\(db, facts\.id\)/g)).toHaveLength(1);
  });

  test('row locks, advisory lock, PMS fragments and day-report/Stripe registries stay untouched', () => {
    const core = read('pos-core.ts');
    const pos = read('pos.ts');
    // r37 folded the fulfillment/register by-id lock heads and r38 folded the sales
    // by-id lock pair into owner-local renderers (still byte-identical, still locked);
    // 17 literal locks remain.
    expect((core.match(/FOR UPDATE/g) ?? []).length).toBe(17);
    expect(core).toContain("const posSaleByIdLockSql = (): string => 'SELECT * FROM pos_sales WHERE id=? FOR UPDATE';");
    expect(core).toContain('FOR UPDATE OF rp,r');
    expect(core).toContain("SELECT pg_advisory_xact_lock(hashtextextended('pos-daily:'||?,0))");
    expect(core).toContain('${pmsProjectionSources.pms_properties}');
    expect(pos).toContain('${pmsProjectionSources.pms_sources}');
    expect(read('native-pos-day-report.ts')).toContain("'pos_day_report_");
    expect(read('native-pos-stripe-reads.ts')).toContain("'pos_stripe_");
  });

  test('retirement:ts-statements is strictly lower than the r33 BEFORE=189 baseline (173 after r42)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(169);
    expect(total).toBeLessThan(189);
  });
});
