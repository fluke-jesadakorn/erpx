import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  nativePosStripeEventStatement,
  nativePosStripeConnectionsReadyStatement,
  nativePosStripeSaleNumberStatement,
  nativePosStripeRefundPaymentStatement,
} from '../../../packages/server-pos/src/adapters/native-pos-stripe-reads';

const srcRoot = resolve(__dirname, '../../../packages/server-pos/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const config = () => JSON.stringify({ privateBaseUrl: 'http://127.0.0.1:44994', timeoutMs: 1000 });
const previous = process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
const rows = [{ id: 'row' }];

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

// Verbatim pre-r35 literals recorded from the AST of `pos-stripe.ts` at
// BEFORE=188. These are byte-identity oracles only, never live query sources.
const cases = [
  {
    name: 'stripe event by id',
    kind: 'pos_stripe_event_by_id',
    sql: 'SELECT * FROM pos_stripe_events WHERE id=?',
    facts: { record_id: 'evt_1' },
    values: ['evt_1'],
    bridge: nativePosStripeEventStatement,
  },
  {
    name: 'ready stripe connection by company/account/livemode',
    kind: 'pos_stripe_connections_ready',
    sql: "SELECT company_id FROM pos_connections WHERE company_id=? AND account_id=? AND livemode=? AND state='ready'",
    facts: { company_id: 'company', accountId: 'acct', fact2: true },
    values: ['company', 'acct', true],
    bridge: nativePosStripeConnectionsReadyStatement,
  },
  {
    name: 'sale number by id',
    kind: 'pos_stripe_sale_number_by_id',
    sql: 'SELECT number FROM pos_sales WHERE id=?',
    facts: { sale_id: 'sale_1' },
    values: ['sale_1'],
    bridge: nativePosStripeSaleNumberStatement,
  },
  {
    name: 'stripe refund payment by refund/company',
    kind: 'pos_stripe_refund_payment_by_refund',
    sql: 'SELECT rp.* FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id WHERE rp.id=? AND r.company_id=?',
    facts: { record_id: 'rp_1', company_id: 'company' },
    values: ['rp_1', 'company'],
    bridge: nativePosStripeRefundPaymentStatement,
  },
] as const;

describe('POS read-tail r35 native Stripe keyed reads', () => {
  it.each(cases)('requests the $name kind with byte parity and ordered binds', async (item) => {
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
    // Byte parity: the SQL executed on the caller's unit is exactly the retired literal.
    expect(seen.sql).toBe(item.sql);
    expect(seen.values).toEqual([...item.values]);
    expect(seen.sql).toMatch(/^SELECT /);
    expect(seen.sql).not.toMatch(/FOR UPDATE|FOR SHARE|FOR NO KEY UPDATE|LOCK TABLE|pg_advisory|pg_try_advisory|INSERT|UPDATE |DELETE|RETURNING|pms_sources|pms_properties|commerce_/);
    // Bind pin: placeholder count equals the positional value count exactly.
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

  it('mounts all four kinds in the Rust registry with byte-identical SQL and bind counts', () => {
    const registry = read('stripe_reads.rs');
    for (const item of cases) {
      expect(registry).toContain(`"${item.sql}"`);
      expect(registry).toContain(`"${item.kind}"`);
      // Bind count oracle in the Rust EXPECTED table matches the TS bridge.
      expect(item.values.length).toBe(item.sql.match(/\?/g)?.length ?? 0);
      expect(item.values.length).toBeGreaterThan(0);
    }
    // Byte parity: the retired literals are gone from the TS adapter.
    const stripe = read('pos-stripe.ts');
    for (const item of cases) expect(stripe).not.toContain(item.sql);
    for (const fn of [
      'nativePosStripeEventStatement', 'nativePosStripeConnectionsReadyStatement',
      'nativePosStripeSaleNumberStatement', 'nativePosStripeRefundPaymentStatement',
    ]) expect(stripe).toMatch(new RegExp(`${fn}<PosRow>\\(db`));
  });

  it('pins the r36 KPIs: stripe kinds 12, sale-child kinds 13, owner kinds 196, TS SELECT tokens 182', () => {
    const registry = read('stripe_reads.rs');
    const owner = read('owner_writes.rs');
    expect(registry).toContain('assert_eq!(KINDS.len(), 12)');
    expect(registry).toContain('assert_eq!(EXPECTED.len(), 12)');
    expect(owner).toContain('assert_eq!(KINDS.len(), 196)');
    expect(owner).toContain('assert_eq!(crate::adapters::stripe_reads::KINDS.len(), 12)');
    expect(owner).toContain('assert_eq!(crate::adapters::sale_child_reads::KINDS.len(), 13)');
    for (const name of ['STRIPE_EVENT_BY_ID', 'CONNECTIONS_READY', 'SALE_NUMBER_BY_ID', 'REFUND_PAYMENT_BY_REFUND'])
      expect(owner).toContain(`crate::adapters::stripe_reads::${name},`);

    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(182);
    expect(total).toBeLessThan(188);
  });

  it('keeps row locks, the aggregate and the context probe inline', () => {
    const stripe = read('pos-stripe.ts');
    expect(stripe).toContain('SELECT * FROM pos_sales WHERE id=? AND company_id=? FOR UPDATE');
    expect(stripe).toContain("SELECT COALESCE(SUM(amount_minor-refunded_minor),0) n FROM pos_payments WHERE sale_id=? AND state='succeeded'");
    expect(stripe).toContain("SELECT to_regclass('pos_jobs') name");
    expect(stripe).toContain("SELECT * FROM pos_oauth_states WHERE id=? AND company_id=? AND user_id=? AND used_at IS NULL AND expires_at>? FOR UPDATE");
    const owner = read('owner_writes.rs');
    expect(owner).toContain('crate::adapters::day_report_reads::SETTLEMENTS');
    expect(owner).toContain('STRIPE_READPOSPAYMENTSBYID_527 => r#"SELECT * FROM pos_payments WHERE id=? FOR UPDATE"#');
  });
});
