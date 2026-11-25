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

// Verbatim pre-r41 oracles recorded from `pos.ts` at BEFORE=175
// (git blob 2509c7ea8b09cccc91721a883d985dc77492fdcd). Byte-identity oracles
// only, never live query sources. The folded head is the refund-payment scalar
// subquery used twice inside `readPosRefundPayments3`: once for the
// `refunded_minor` column and once inside `contribution_minor`. It is
// byte-identical at both sites (213 bytes, 1 SELECT, zero binds, no row lock)
// and reads only POS-owned tables (pos_refund_payments / pos_refunds). The
// zero-arg owner-local renderer returns the exact literal, so the fully
// rendered payment statement keeps its 3 SELECTs, 7 binds and original byte
// sequence. This is a new family beyond r35-r40: r40 folded the differently
// indented `::numeric`-less variant in executive-evidence-sources.ts, not this
// `::text` variant in pos.ts. No r35-r40 native/lock/renderer head, sale-child
// or owner_writes registry, advisory lock, PMS fragment, day-report/Stripe
// registry, settlement mirror or cross-owner projection is touched.
const HEAD = `SELECT sum(rp.amount_minor)::numeric FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id AND r.company_id=p.company_id
       WHERE rp.payment_id=p.id AND rp.state='succeeded' AND r.kind='sale'),0)`;
const HEAD_SHA = '62e967ba3f57343cfef8792e7481151512e24c32645c50f0490e5aeb0b346d75';
const RENDERED_FULL = `SELECT p.id source_id,'payment' source_type,s.id sale_id,s.number receipt_number,
     s.business_date,NULL::text accounting_date,s.currency,s.state sale_state,p.state,p.method,p.amount_minor::text amount_minor,
     COALESCE((SELECT sum(rp.amount_minor)::numeric FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id AND r.company_id=p.company_id
       WHERE rp.payment_id=p.id AND rp.state='succeeded' AND r.kind='sale'),0)::text refunded_minor,
     (p.amount_minor::numeric-COALESCE((SELECT sum(rp.amount_minor)::numeric FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id AND r.company_id=p.company_id
       WHERE rp.payment_id=p.id AND rp.state='succeeded' AND r.kind='sale'),0))::text contribution_minor,
     p.version revision,s.version sale_version,s.branch_id,p.created_at
   FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id AND s.company_id=p.company_id
   WHERE p.id=? AND p.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=?
     AND s.business_date BETWEEN ? AND ? AND s.state='completed' AND p.state='succeeded' LIMIT 1`;
const RENDERED_FULL_SHA = '5299bafa4da2d4476f7df990b4214281e0ca2152148c92a11f214b8c506636d5';
const RENDERER = 'const refundPaymentSumTextSql = (): string => `';
const CALL = '${refundPaymentSumTextSql()}';

describe('POS read-tail r41 refund-payment text subquery dedupe', () => {
  test('fold: byte-identical head is authored once in a zero-arg owner-local renderer', () => {
    const pos = read('pos.ts');
    expect(pos).toContain(RENDERER + HEAD + '`');
    expect(occurrences(pos, HEAD)).toBe(1);
    expect(sha(HEAD)).toBe(HEAD_SHA);
    expect(HEAD.length).toBe(213);
    expect(selects(HEAD)).toBe(1);
    expect(binds(HEAD)).toBe(0);
    expect(HEAD.startsWith('SELECT sum(rp.amount_minor)::numeric FROM pos_refund_payments rp')).toBe(true);
    expect(HEAD.endsWith("AND r.kind='sale'),0)")).toBe(true);
    expect(HEAD).not.toContain('FOR UPDATE');
    expect(HEAD.endsWith('\n')).toBe(false);
  });

  test('both sites delegate; rendered payment statement stays byte-identical with 3 SELECTs and 7 binds', () => {
    const pos = read('pos.ts');
    expect(occurrences(pos, CALL)).toBe(2);
    expect(pos).toContain(`COALESCE((${CALL}::text refunded_minor,`);
    expect(pos).toContain(`(p.amount_minor::numeric-COALESCE((${CALL})::text contribution_minor,`);
    const start = pos.indexOf('readPosRefundPayments3:');
    expect(start).toBeGreaterThanOrEqual(0);
    const bodyStart = pos.indexOf('`', start) + 1;
    const bodyEnd = pos.indexOf('`', bodyStart);
    const rendered = pos.slice(bodyStart, bodyEnd).split(CALL).join(HEAD);
    expect(rendered).toBe(RENDERED_FULL);
    expect(sha(rendered)).toBe(RENDERED_FULL_SHA);
    expect(selects(rendered)).toBe(3);
    expect(binds(rendered)).toBe(7);
    expect(pos).toContain('.bind(facts.id, facts.company, facts.branch, facts.branch2, facts.currency, facts.from, facts.to)');
  });

  test('protected r35-r40 renderers, registries, advisory locks, PMS fragments, settlement mirrors and lock tails stay untouched', () => {
    const core = read('pos-core.ts');
    expect(core).toContain("const posSaleByIdLockSql = (): string => 'SELECT * FROM pos_sales WHERE id=? FOR UPDATE';");
    expect(core).toContain('const posFulfillmentByIdLockSql = (): string =>');
    expect(core).toContain('const posRegisterByIdLockSql = (): string =>');
    expect((core.match(/FOR UPDATE/g) ?? []).length).toBe(17);
    expect(core).toContain('FOR UPDATE OF rp,r');
    expect(core).toContain("SELECT pg_advisory_xact_lock(hashtextextended('pos-daily:'||?,0))");
    expect(core).toContain('${pmsProjectionSources.pms_properties}');
    expect(read('pos.ts')).toContain('${pmsProjectionSources.pms_sources}');
    expect(read('executive-evidence-sources.ts')).toContain('const refundPaymentSumSql = (): string => `');
    expect(read('executive-evidence-sources.ts')).toContain('const posSalesLegSql = (): string => `');
    expect(read('pos.ts')).toContain('const allocatedTotalsHeadSql = (): string => `WITH allocated AS (');
    expect(read('native-pos-day-report.ts')).toContain("'pos_day_report_");
    expect(read('native-pos-stripe-reads.ts')).toContain("'pos_stripe_");
    expect(read('pos-stripe.ts')).toContain('SELECT * FROM pos_sales WHERE id=? AND company_id=? FOR UPDATE');
    expect(read('owner_writes.rs')).toContain('assert_eq!(KINDS.len(), 196)');
    expect(read('owner_writes.rs')).toContain('assert_eq!(crate::adapters::sale_child_reads::KINDS.len(), 13)');
    expect(read('addon-package-obligations.ts')).toContain('UNION SELECT s.package_id,s.company_id,');
  });

  test('retirement:ts-statements decreased 175 -> 173 (r42 continues below the r41 BEFORE=175 baseline)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(172);
    expect(total).toBeLessThan(175);
  });
});
