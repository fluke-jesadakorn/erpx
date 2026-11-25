import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { describe, expect, it, vi } from 'vitest';
import { ApiError } from '@erpx/server-runtime/public/http';

vi.mock('server-only', () => ({}));

import {
  createNativeLane38Client,
  nativeBillingWatermarks,
  type NativeLane38Context,
  type NativeLane38Transport,
} from '../../../packages/server-billing/src/adapters/native-lane-38/client';
import { billingWatermarkRevisionSql as revision } from '../../../packages/server-billing/src/adapters/billing-read-tail-r22-sql';
import { billingPointReadSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r29-sql';
import { billingProjectionSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r35-sql';

const host = 'acme.jesadakorn.com';
const origin = `https://${host}`;
const holdingId = '11111111-2222-3333-4444-555555555555';
const holding = { origin, host, holdingId, holdingSlug: 'acme' };
const context: NativeLane38Context = { holdingId, holdingSlug: 'acme' };
const config = (
  privateBaseUrl: string,
  holdings: unknown[] = [holding],
  timeoutMs: number = 1000,
) => JSON.stringify({ privateBaseUrl, timeoutMs, holdings });

type Seen = { url: URL; headers: Record<string, string>; timeoutMs: number };
const transport = (
  status: number,
  value: unknown,
  seen?: Seen,
  throwError = false,
): NativeLane38Transport => async (url, headers, timeoutMs) => {
  if (seen) Object.assign(seen, { url, headers, timeoutMs });
  if (throwError) throw new Error('offline');
  return { status, value };
};

const apiError = async (input: Promise<unknown> | (() => Promise<unknown>)) => {
  try {
    await (typeof input === 'function' ? input() : input);
    throw new Error('expected rejection');
  } catch (error) {
    expect(error).toBeInstanceOf(ApiError);
    return error as { status: number; code: string };
  }
};

describe('native lane 38 billing watermark transport', () => {
  it('fails closed with 503 when the native owner is not configured', async () => {
    const error = await apiError(
      () => createNativeLane38Client('', transport(200, {})).watermarks(context),
    );
    expect(error.status).toBe(503);
    expect(error.code).toBe('NATIVE_LANE_38_BILLING_CONFIG_REQUIRED');
  });

  it('fails closed on a misconfigured transport', async () => {
    const bad = config('http://127.0.0.1:3100');
    const error = await apiError(
      () => createNativeLane38Client(bad, transport(200, {})).watermarks(context),
    );
    expect(error.status).toBe(503);
    expect(error.code).toBe('NATIVE_LANE_38_BILLING_CONFIG_INVALID');
  });

  it('rejects 0 and 65 configured holdings', async () => {
    for (const holdings of [[], Array.from({ length: 65 }, (_, index) => ({ ...holding, holdingId: `h${index}` }))]) {
      const error = await apiError(() =>
        createNativeLane38Client(config('http://127.0.0.1:3401', holdings), transport(200, {})).watermarks(context),
      );
      expect(error.status).toBe(503);
      expect(error.code).toBe('NATIVE_LANE_38_BILLING_CONFIG_INVALID');
    }
  });

  it('selects exactly one holding from the caller WorkContext and pins the response shape', async () => {
    const seen: Seen = {} as Seen;
    const revisionValue = 'a'.repeat(32);
    const client = createNativeLane38Client(
      config('http://127.0.0.1:3401'),
      transport(200, { watermarks: [{ table_name: 'billing_snapshot', revision: revisionValue }] }, seen),
    );
    const result = await client.watermarks(context);
    expect(result).toEqual([{ table_name: 'billing_snapshot', revision: revisionValue }]);
    expect(seen.url.pathname).toBe('/api/v1/holdings/acme/billing/native-lane-38/watermarks');
    expect(seen.headers).toMatchObject({
      host,
      origin,
      accept: 'application/json',
      'x-erpx-context-company': holdingId,
    });
    expect(seen.timeoutMs).toBe(1000);
    expect(createHash('sha256').update(JSON.stringify(result)).digest('hex')).toBe(
      '8ff2467b7f46529b16e328f325fb20c8001504f30ec36c3f9767e8a2a32f3b62',
    );
  });

  it('never defaults to a first holding and fails closed on a context mismatch', async () => {
    const second = {
      origin: 'https://northstar.jesadakorn.com',
      host: 'northstar.jesadakorn.com',
      holdingId: 'northstar',
      holdingSlug: 'northstar',
    };
    const seen: Seen = {} as Seen;
    const client = createNativeLane38Client(
      config('http://127.0.0.1:3401', [holding, second]),
      transport(200, { watermarks: [] }, seen),
    );
    await client.watermarks({ holdingId: 'northstar', holdingSlug: 'northstar' });
    expect(seen.url.pathname).toBe('/api/v1/holdings/northstar/billing/native-lane-38/watermarks');
    expect(seen.headers.host).toBe(second.host);
    const wrongSlug = await apiError(() =>
      client.watermarks({ holdingId: 'northstar', holdingSlug: 'acme' }),
    );
    expect(wrongSlug.status).toBe(422);
    // A bare holding id that is not unique never resolves to a first match.
    const ambiguous = createNativeLane38Client(
      config('http://127.0.0.1:3401', [{ ...holding, holdingSlug: 'acme' }, { ...holding, holdingSlug: 'acme-alt' }]),
      transport(200, { watermarks: [] }),
    );
    const error = await apiError(() => ambiguous.watermarks(holdingId));
    expect(error.status).toBe(422);
  });

  it('uses the optional companyId as the owner company scope', async () => {
    const seen: Seen = {} as Seen;
    const client = createNativeLane38Client(
      config(
        'http://127.0.0.1:3401',
        [{ ...holding, companyId: 'company-9' }],
      ),
      transport(200, { watermarks: [] }, seen),
    );
    await client.watermarks({ ...context, companyId: 'company-9' });
    expect(seen.headers['x-erpx-context-company']).toBe('company-9');
  });

  it('fails closed on malformed owner replies', async () => {
    const replies: unknown[] = [
      null,
      {},
      { watermarks: 'x' },
      { watermarks: [{ table_name: 'wrong', revision: 'abc' }] },
      { watermarks: [{ table_name: 'billing_snapshot', revision: 5 }] },
      { watermarks: [{ table_name: 'billing_snapshot', revision: 'abc', extra: 1 }] },
    ];
    for (const reply of replies) {
      const error = await apiError(() =>
        createNativeLane38Client(config('http://127.0.0.1:3401'), transport(200, reply)).watermarks(context),
      );
      expect(error.status).toBe(503);
      expect(error.code).toBe('NATIVE_LANE_38_BILLING_WATERMARK_RESPONSE_INVALID');
    }
  });

  it('fails closed on transport failure and non-200 replies', async () => {
    const offline = await apiError(() =>
      createNativeLane38Client(
        config('http://127.0.0.1:3401'),
        transport(200, {}, undefined, true),
      ).watermarks(context),
    );
    expect(offline.status).toBe(503);
    const unavailable = await apiError(() =>
      createNativeLane38Client(
        config('http://127.0.0.1:3401'),
        transport(503, { error: { code: 'X', message: 'down' } }),
      ).watermarks(context),
    );
    expect(unavailable.status).toBe(503);
    expect(unavailable.code).toBe('X');
    const rejected = await apiError(() =>
      createNativeLane38Client(config('http://127.0.0.1:3401'), transport(422, {})).watermarks(context),
    );
    expect(rejected.status).toBe(422);
  });

  it('rejects an unbounded WorkContext before attempting any call', async () => {
    const error = await apiError(() =>
      createNativeLane38Client(config('http://127.0.0.1:3401'), transport(200, {})).watermarks(
        { holdingId, holdingSlug: 'Bad' },
      ),
    );
    expect(error.status).toBe(422);
    expect(error.code).toBe('NATIVE_LANE_38_BILLING_WATERMARK_REQUEST_INVALID');
  });

  it('the env-bound reader fails closed without configuration', async () => {
    const previous = process.env.ERPX_NATIVE_BILLING_CONFIG;
    delete process.env.ERPX_NATIVE_BILLING_CONFIG;
    try {
      const error = await apiError(() => nativeBillingWatermarks(context));
      expect(error.status).toBe(503);
    } finally {
      if (previous === undefined) delete process.env.ERPX_NATIVE_BILLING_CONFIG;
      else process.env.ERPX_NATIVE_BILLING_CONFIG = previous;
    }
  });
});

describe('native lane 38 owner oracle and dev configuration', () => {
  it('renders byte-identical owner statement text whose oracle hash is pinned', () => {
    const columns = `a.version::text account_version,
         ${revision('e.id,e.plan_id,e.started_at,e.ended_at,e.pending_end,e.pending_plan,e.effective_at', 'e.id', 'billing_enrollments e', 'e.holding_id=a.holding_id')} enrollment_revision,
         ${revision('i.id,i.status,i.due_at,i.total_satang,i.updated_at,i.kind', 'i.id', 'billing_invoices i', 'i.holding_id=a.holding_id')} invoice_revision,
         ${revision('b.subscription_id,b.status,b.current_period_end,b.last_event_at,b.updated_at', 'b.holding_id', 'beta_billing b', 'b.holding_id=a.holding_id')} legacy_subscription_revision,
         ${revision('p.id,p.revision,p.name,p.effective_month', 'p.id,p.revision', 'billing_plans p', `p.id IN (${billingPointReadSql('e.plan_id', 'billing_enrollments e', 'e.holding_id=a.holding_id')})`)} plan_revision
       `;
    const sql = billingProjectionSql(columns, 'FROM billing_accounts a WHERE a.holding_id=?');
    expect(createHash('sha256').update(sql).digest('hex')).toBe(
      '943cf18a6b43ea0d2ccc69f3267316218b78ab2542c660aa5796a92c14f35d72',
    );
    expect(sql.split('SELECT').length - 1).toBe(6);
  });

  it('the Rust owner contains the rendered statement with the single sqlx bind', async () => {
    const rust = await readFile(
      new URL('../../../packages/server-billing/src/adapters/native_lane_38/postgres.rs', import.meta.url),
      'utf8',
    );
    expect(rust).toContain('FROM billing_accounts a WHERE a.holding_id=$1');
    expect(rust).toContain('md5(COALESCE(string_agg(jsonb_build_array(e.id');
    expect(rust).toContain('plan_revision');
  });

  it('the retained TS adapter issues no statement text and delegates to the native client', async () => {
    const adapter = await readFile(
      new URL('../../../packages/server-billing/src/adapters/executive-metric-watermarks.ts', import.meta.url),
      'utf8',
    );
    expect(adapter).toContain("from './native-lane-38/client'");
    expect(adapter).toContain('nativeBillingWatermarks(context)');
    expect(adapter).not.toContain('.prepare(');
    expect(adapter).not.toContain('billingProjectionSql');
    expect(adapter).not.toContain('billingWatermarkRevisionSql');
    expect(adapter).not.toContain('SELECT');
  });

  it('the exact dev config is accepted with three registered holdings', async () => {
    // Mirrors .scratch/wave0/lane_38/dev-env.json, kept inline so this contract
    // test does not depend on ignored scratch files.
    const dev = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:3198',
      timeoutMs: 10000,
      holdings: [
        { origin: 'https://acme.jesadakorn.com', host: 'acme.jesadakorn.com', holdingId: 'acme', holdingSlug: 'acme' },
        { origin: 'https://northstar.jesadakorn.com', host: 'northstar.jesadakorn.com', holdingId: 'northstar', holdingSlug: 'northstar' },
        { origin: 'https://erpx-internal.jesadakorn.com', host: 'erpx-internal.jesadakorn.com', holdingId: 'erpx', holdingSlug: 'erpx' },
      ],
    });
    const seen: Seen = {} as Seen;
    const client = createNativeLane38Client(dev, transport(200, { watermarks: [] }, seen));
    await client.watermarks({ holdingId: 'erpx', holdingSlug: 'erpx' });
    expect(seen.url.pathname).toBe('/api/v1/holdings/erpx/billing/native-lane-38/watermarks');
    expect(seen.headers.host).toBe('erpx-internal.jesadakorn.com');
  });
});
