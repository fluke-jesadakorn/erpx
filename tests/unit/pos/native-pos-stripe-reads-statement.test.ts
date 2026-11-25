import { describe, it, expect, vi, afterEach } from 'vitest';
import { readFileSync } from 'node:fs';
vi.mock('server-only', () => ({}));
import {
  nativePosSettlementStateStatement,
  nativePosSettlementBalanceStateStatement,
  nativePosSettlementPayoutJournalStatement,
  nativePosSettlementBalanceRowStatement,
  nativePosStripePaymentStatement,
  nativePosStripePaymentIdStatement,
  nativePosStripeRefundStatement,
  nativePosStripeRefundIdStatement,
  nativePosSettlementByJournalStatement,
  nativePosStripeJobsContextStatement,
} from '../../../packages/server-pos/src/adapters/native-pos-stripe-reads';

const config = () => JSON.stringify({ privateBaseUrl: 'http://127.0.0.1:44994', timeoutMs: 1000 });
const previous = process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
const rows = [{ id: 'row' }];
const scope = { company_id: 'company', account_id: 'acct', livemode: true };

afterEach(() => {
  vi.unstubAllGlobals();
  if (previous === undefined) delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
  else process.env.ERPX_NATIVE_POS_OWNER_CONFIG = previous;
});

function mockDb(seen: { sql?: string; values?: unknown[] }) {
  return {
    prepare: (sql: string) => ({
      bind: (...values: unknown[]) => {
        seen.sql = sql;
        seen.values = values;
        return {
          all: async () => ({ results: rows, success: true, meta: { changes: 0, duration: 0 } }),
          first: async () => rows[0],
        };
      },
    }),
  };
}

const cases = [
  {
    name: 'balance settlement by company/account/livemode/key',
    kind: 'pos_stripe_settlement_by_key',
    sql: "SELECT state FROM pos_settlements WHERE company_id=? AND account_id=? AND livemode=? AND kind='balance' AND provider_id=?",
    facts: { ...scope, btId: 'txn_1' },
    values: ['company', 'acct', true, 'txn_1'],
    bridge: nativePosSettlementStateStatement,
  },
  {
    name: 'balance settlement state by account/livemode/provider',
    kind: 'pos_stripe_settlement_balance_state',
    sql: "SELECT state FROM pos_settlements WHERE account_id=? AND livemode=? AND kind='balance' AND provider_id=?",
    facts: { account_id: 'acct', livemode: true, fact2: 'txn_1' },
    values: ['acct', true, 'txn_1'],
    bridge: nativePosSettlementBalanceStateStatement,
  },
  {
    name: 'payout settlement journal by company/account/livemode/provider',
    kind: 'pos_stripe_settlement_payout_journal',
    sql: "SELECT journal_id FROM pos_settlements WHERE company_id=? AND account_id=? AND livemode=? AND kind='payout' AND provider_id=? AND journal_id IS NOT NULL",
    facts: { ...scope, id: 'po_1' },
    values: ['company', 'acct', true, 'po_1'],
    bridge: nativePosSettlementPayoutJournalStatement,
  },
  {
    name: 'balance settlement row by account/livemode/provider',
    kind: 'pos_stripe_settlement_balance_row',
    sql: "SELECT * FROM pos_settlements WHERE account_id=? AND livemode=? AND kind='balance' AND provider_id=?",
    facts: { ...scope, id: 'txn_1' },
    values: ['acct', true, 'txn_1'],
    bridge: nativePosSettlementBalanceRowStatement,
  },
  {
    name: 'payment by id scoped to Stripe account',
    kind: 'pos_stripe_payment_by_id',
    sql: 'SELECT * FROM pos_payments WHERE id=? AND company_id=? AND stripe_account=? AND livemode=?',
    facts: { ...scope, pid: 'pay_1' },
    values: ['pay_1', 'company', 'acct', true],
    bridge: nativePosStripePaymentStatement,
  },
  {
    name: 'payment id scoped to Stripe account',
    kind: 'pos_stripe_payment_id',
    sql: 'SELECT id FROM pos_payments WHERE id=? AND company_id=? AND stripe_account=? AND livemode=?',
    facts: { ...scope, paymentId: 'pay_1' },
    values: ['pay_1', 'company', 'acct', true],
    bridge: nativePosStripePaymentIdStatement,
  },
  {
    name: 'refund by id scoped to Stripe account',
    kind: 'pos_stripe_refund_by_id',
    sql: 'SELECT rp.* FROM pos_refund_payments rp JOIN pos_payments p ON p.id=rp.payment_id WHERE rp.id=? AND p.company_id=? AND p.stripe_account=? AND p.livemode=?',
    facts: { ...scope, rpid: 'ref_1' },
    values: ['ref_1', 'company', 'acct', true],
    bridge: nativePosStripeRefundStatement,
  },
  {
    name: 'refund id by provider source scoped to Stripe account',
    kind: 'pos_stripe_refund_id_by_provider',
    sql: 'SELECT rp.id FROM pos_refund_payments rp JOIN pos_payments p ON p.id=rp.payment_id WHERE rp.provider_id=? AND p.company_id=? AND p.stripe_account=? AND p.livemode=?',
    facts: { ...scope, source: 're_1' },
    values: ['re_1', 'company', 'acct', true],
    bridge: nativePosStripeRefundIdStatement,
  },
  {
    name: 'settlement id by Finance journal (idempotency guard)',
    kind: 'pos_stripe_settlement_by_journal',
    sql: 'SELECT id FROM pos_settlements WHERE journal_id=?',
    facts: 'journal_1',
    values: ['journal_1'],
    bridge: nativePosSettlementByJournalStatement,
  },
  {
    name: 'owned pos_jobs catalog probe (parameterized table name)',
    kind: 'pos_stripe_jobs_context',
    sql: 'SELECT to_regclass(?) name',
    facts: {},
    values: ['pos_jobs'],
    bridge: nativePosStripeJobsContextStatement,
  },
] as const;

describe('native POS Stripe settlement/provider read bridge', () => {
  it.each(cases)('requests the $name from the native owner registry only', async (item) => {
    process.env.ERPX_NATIVE_POS_OWNER_CONFIG = config();
    const fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ data: { sql: item.sql, values: item.values } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );
    vi.stubGlobal('fetch', fetch);
    const seen: { sql?: string; values?: unknown[] } = {};
    const result = await (item.bridge as (db: never, facts: never) => Promise<unknown>)(
      mockDb(seen) as never, item.facts as never,
    );
    expect(result).toEqual(rows[0]);
    expect(fetch).toHaveBeenCalledTimes(1);
    const [url, init] = fetch.mock.calls[0];
    expect(url).toBe('http://127.0.0.1:44994/api/v1/companies//pos/owner-write');
    expect(init.method).toBe('POST');
    expect(init.headers['x-erpx-context-company']).toBe('');
    expect(init.headers['idempotency-key']).toMatch(/^[0-9a-f-]{36}$/);
    expect(JSON.parse(init.body)).toEqual({ company: '', kind: item.kind, values: [...item.values] });
    expect(seen.sql).toBe(item.sql);
    expect(seen.values).toEqual([...item.values]);
    expect(seen.sql).toMatch(/^SELECT /);
    expect(seen.sql).not.toMatch(/FOR UPDATE|FOR SHARE|FOR NO KEY UPDATE|LOCK TABLE|pg_advisory|INSERT|UPDATE |DELETE|RETURNING|pms_sources|pms_properties/);
    expect(seen.sql!.match(/\?/g)?.length).toBe(item.values.length);
  });

  it.each(cases)('fails closed for $name when the native owner service is unconfigured', async (item) => {
    delete process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    await expect(
      (item.bridge as (db: never, facts: never) => Promise<unknown>)({} as never, item.facts as never),
    ).rejects.toMatchObject({ code: 'NATIVE_POS_OWNER_UNAVAILABLE', status: 503 });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('keeps no retired Stripe read SQL literal in the TS adapter', () => {
    const stripe = readFileSync(new URL('../../../packages/server-pos/src/adapters/pos-stripe.ts', import.meta.url), 'utf8');
    for (const item of cases) expect(stripe).not.toContain(item.sql);
    for (const fn of [
      'nativePosSettlementStateStatement', 'nativePosSettlementBalanceStateStatement',
      'nativePosSettlementPayoutJournalStatement', 'nativePosSettlementBalanceRowStatement',
      'nativePosStripePaymentStatement', 'nativePosStripePaymentIdStatement',
      'nativePosStripeRefundStatement', 'nativePosStripeRefundIdStatement',
      'nativePosStripeJobsContextStatement',
    ]) expect(stripe).toMatch(new RegExp(`${fn}(?:<PosRow>)?\\(db`));
    const core = readFileSync(new URL('../../../packages/server-pos/src/adapters/pos-core.ts', import.meta.url), 'utf8');
    expect(core).not.toContain('SELECT id FROM pos_settlements WHERE journal_id=?');
    expect(core).toContain('nativePosSettlementByJournalStatement(db, facts.journalId)');
    expect(stripe).not.toContain("to_regclass('pos_jobs')");
    expect(stripe).toContain('readContext1: () => nativePosStripeJobsContextStatement(db)');
  });

  it('keeps row locks and the day-report registry inline', () => {
    const stripe = readFileSync(new URL('../../../packages/server-pos/src/adapters/pos-stripe.ts', import.meta.url), 'utf8');
    expect(stripe).toContain("SELECT * FROM pos_oauth_states WHERE id=? AND company_id=? AND user_id=? AND used_at IS NULL AND expires_at>? FOR UPDATE");
    expect(stripe).toContain('SELECT * FROM pos_sales WHERE id=? AND company_id=? FOR UPDATE');
    const owner = readFileSync(new URL('../../../packages/server-pos/src/adapters/owner_writes.rs', import.meta.url), 'utf8');
    expect(owner).toContain('crate::adapters::day_report_reads::SETTLEMENTS');
    expect(owner).toContain('STRIPE_READPOSPAYMENTSBYID_527 => r#"SELECT * FROM pos_payments WHERE id=? FOR UPDATE"#');
  });
});
