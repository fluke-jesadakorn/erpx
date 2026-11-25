import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

const srcRoot = resolve(__dirname, '../../../packages/server-pos/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const occurrences = (text: string, needle: string) => text.split(needle).length - 1;
const binds = (s: string) => (s.match(/\?/g) ?? []).length;
const selects = (s: string) => (s.match(/SELECT /g) ?? []).length;
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Verbatim pre-r40 oracles recorded from the AST at BEFORE=176
// (HEAD 02df0198169327083838b0f773e78a853650800b). Byte-identity oracles only,
// never live query sources. The folded head is the refund-payment scalar
// subquery used twice inside the pos-payment-mix evidence leg: once for the
// contribution_minor offset and once for refunded_minor. It is byte-identical
// at both sites (223 bytes, 1 SELECT, zero binds, no row lock). The zero-arg
// owner-local renderer returns the exact literal, so the fully rendered
// payment leg keeps its 9 binds, 3 SELECTs and original byte sequence. No
// r35-r39 native/lock/renderer head, sale-child/owner_writes registry, advisory
// lock, PMS fragment or addon settlement mirror is touched.
const ORACLE_BLOCK = `(SELECT sum(rp.amount_minor)::numeric FROM pos_refund_payments rp
        JOIN pos_refunds r ON r.id=rp.refund_id AND r.company_id=p.company_id
        WHERE rp.payment_id=p.id AND rp.state='succeeded' AND r.kind='sale'),0)`;
const ORACLE_PAYMENT_LEG = `SELECT 'payment:'||p.id id,p.id source_id,'payment' source_type,s.id sale_id,s.business_date event_date,
      s.number receipt_number,p.state status,s.state sale_status,s.currency,p.method,
      p.amount_minor::numeric amount_minor,
      (p.amount_minor::numeric-COALESCE((SELECT sum(rp.amount_minor)::numeric FROM pos_refund_payments rp
        JOIN pos_refunds r ON r.id=rp.refund_id AND r.company_id=p.company_id
        WHERE rp.payment_id=p.id AND rp.state='succeeded' AND r.kind='sale'),0)) contribution_minor,
      COALESCE((SELECT sum(rp.amount_minor)::numeric FROM pos_refund_payments rp
        JOIN pos_refunds r ON r.id=rp.refund_id AND r.company_id=p.company_id
        WHERE rp.payment_id=p.id AND rp.state='succeeded' AND r.kind='sale'),0) refunded_minor,
      p.version revision,s.version sale_version
    FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id AND s.company_id=p.company_id
    WHERE p.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=?
      AND s.business_date BETWEEN ? AND ? AND s.state='completed' AND p.state='succeeded'
      AND (?='' OR position(lower(?) in lower(s.number))>0 OR position(lower(?) in lower(p.method))>0)`;
const BLOCK_SHA = '9ce4c9b2269cc7f63b89d8644d0802f80f43033923041ee856dc0df6eafda22b';
const PAYMENT_LEG_SHA = '884d6b57c718cea724ddfb722bda04c9a825fb9a53ce52e6db91c5a12507a49b';
const RENDERER = 'const refundPaymentSumSql = (): string => ';
const CALL = '${refundPaymentSumSql()}';

describe('POS read-tail r40 refund-payment subquery dedupe', () => {
  test('fold: byte-identical head is authored once in the native owner (lane 25)', () => {
    const rust = read('native_lane_25/postgres.rs');
    const ev = read('executive-evidence-sources.ts');
    expect(occurrences(rust, ORACLE_BLOCK)).toBe(2);
    expect(sha(ORACLE_BLOCK)).toBe(BLOCK_SHA);
    expect(ORACLE_BLOCK.length).toBe(223);
    expect(binds(ORACLE_BLOCK)).toBe(0);
    expect(selects(ORACLE_BLOCK)).toBe(1);
    expect(ORACLE_BLOCK.startsWith('(SELECT sum(rp.amount_minor)::numeric FROM pos_refund_payments rp')).toBe(true);
    expect(ORACLE_BLOCK.endsWith("AND r.kind='sale'),0)")).toBe(true);
    expect(ORACLE_BLOCK).not.toContain('FOR UPDATE');
    expect(ORACLE_BLOCK.endsWith('\n')).toBe(false);
  });

  test('native owner renders payment SQL byte-identically with 9 binds and 3 SELECTs', () => {
    const rust = read('native_lane_25/postgres.rs');
    const ev = read('executive-evidence-sources.ts');
    expect(rust).toContain('pub const LEG_PAYMENT: &str = r#"SELECT \'payment:\'||p.id id');
    expect(rust).toContain(ORACLE_PAYMENT_LEG);
    expect(sha(ORACLE_PAYMENT_LEG)).toBe(PAYMENT_LEG_SHA);
    expect(binds(ORACLE_PAYMENT_LEG)).toBe(9);
    expect(selects(ORACLE_PAYMENT_LEG)).toBe(3);
    expect(ev).not.toContain(RENDERER);
    expect(ev).not.toContain(CALL);
    expect(ev).toContain('nativePosEvidenceRows');
  });

  test('protected r35-r39 renderers, registries, advisory locks, PMS fragments, addon mirrors and lock tails stay untouched', () => {
    const core = read('pos-core.ts');
    expect(core).toContain("const posSaleByIdLockSql = (): string => 'SELECT * FROM pos_sales WHERE id=? FOR UPDATE';");
    expect((core.match(/FOR UPDATE/g) ?? []).length).toBe(17);
    expect(core).toContain('FOR UPDATE OF rp,r');
    expect(core).toContain("SELECT pg_advisory_xact_lock(hashtextextended('pos-daily:'||?,0))");
    expect(core).toContain('${pmsProjectionSources.pms_properties}');
    expect(read('pos.ts')).toContain('const allocatedTotalsHeadSql = (): string => `WITH allocated AS (');
    expect(read('pos.ts')).toContain('${pmsProjectionSources.pms_sources}');
    expect(read('executive-evidence-sources.ts')).toContain('nativePosEvidenceRows');
    expect(read('executive-evidence-sources.ts')).not.toMatch(/\bSELECT\b/);
    expect(read('native_lane_25/postgres.rs')).toContain('LEG_POS_SALES');
    expect(read('native-pos-day-report.ts')).toContain("'pos_day_report_");
    expect(read('native-pos-stripe-reads.ts')).toContain("'pos_stripe_");
    expect(read('pos-stripe.ts')).toContain('SELECT * FROM pos_sales WHERE id=? AND company_id=? FOR UPDATE');
    expect(read('owner_writes.rs')).toContain('assert_eq!(KINDS.len(), 197)');
    expect(read('owner_writes.rs')).toContain('assert_eq!(crate::adapters::sale_child_reads::KINDS.len(), 13)');
    expect(read('addon-package-obligations.ts')).toContain('UNION SELECT s.package_id,s.company_id,');
  });

  test('retirement:ts-statements decreased 176 -> 173 (r42 continues below the r40 BEFORE=176 baseline)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(158);
    expect(total).toBeLessThan(176);
  });
});
