import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));

import {
  createFailedMetricsClient,
  nativeWorkflowLane32Configuration,
  readFailedMetricsReply,
  selectHolding,
} from '../../../modules/workflow/server/src/adapters/native-lane-32/client';

const holding = (overrides: Record<string, unknown> = {}) => ({
  origin: 'https://acme.example.com',
  host: 'acme.example.com',
  holdingId: 'h001',
  holdingSlug: 'acme',
  ...overrides,
});
const northstar = holding({
  origin: 'https://northstar.example.com',
  host: 'northstar.example.com',
  holdingId: 'h002',
  holdingSlug: 'northstar',
});
const configRaw = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:44995',
  timeoutMs: 1000,
  holdings: [holding(), northstar],
});
const ok = (value: unknown) => Promise.resolve({ status: 200, value });
const sha = (value: string) => createHash('sha256').update(value).digest('hex');

const fails = (run: () => unknown, code: string) => {
  try {
    run();
  } catch (error) {
    expect(error).toMatchObject({ status: 503, code });
    return;
  }
  throw new Error(`expected ApiError ${code}`);
};

/** Exact registered development configuration (Holding registry IDs/hosts). */
export const DEV_LANE32_CONFIG_JSON = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:3198',
  timeoutMs: 10000,
  holdings: [
    { holdingId: 'h001', holdingSlug: 'acme', host: 'acme.jesadakorn.com', origin: 'https://acme.jesadakorn.com' },
    { holdingId: 'h002', holdingSlug: 'northstar', host: 'northstar.jesadakorn.com', origin: 'https://northstar.jesadakorn.com' },
    { holdingId: 'h003', holdingSlug: 'erpx', host: 'erpx-internal.jesadakorn.com', origin: 'https://erpx-internal.jesadakorn.com' },
  ],
});

const SOURCE_SQL = 'SELECT count(*)::int count FROM outbox_events WHERE dead_letter_at IS NOT NULL';
const EXECUTED_SQL = 'SELECT count(*)::bigint count FROM outbox_events WHERE dead_letter_at IS NOT NULL';

describe('native lane-32 Workflow failed-metrics client', () => {
  it('accepts the exact registered dev config on 127.0.0.1:3198', () => {
    const cfg = nativeWorkflowLane32Configuration(DEV_LANE32_CONFIG_JSON);
    expect(cfg.base).toBe('http://127.0.0.1:3198');
    expect(cfg.timeoutMs).toBe(10000);
    expect(cfg.holdings.map(held => [held.holdingId, held.holdingSlug, held.host])).toEqual([
      ['h001', 'acme', 'acme.jesadakorn.com'],
      ['h002', 'northstar', 'northstar.jesadakorn.com'],
      ['h003', 'erpx', 'erpx-internal.jesadakorn.com'],
    ]);
    for (const held of cfg.holdings) {
      expect(selectHolding(cfg, { holdingId: held.holdingId, holdingSlug: held.holdingSlug })).toBe(held);
      expect(selectHolding(cfg, { origin: held.origin })).toBe(held);
    }
  });

  it('accepts only registered, distinct private Holding targets', () => {
    const cfg = nativeWorkflowLane32Configuration(configRaw);
    expect(cfg.holdings).toHaveLength(2);
    expect(cfg.holdings[1]).toMatchObject({
      host: 'northstar.example.com',
      holdingSlug: 'northstar',
      origin: 'https://northstar.example.com',
    });
    fails(() => nativeWorkflowLane32Configuration(''), 'WORKFLOW_FAILED_METRICS_READ_UNAVAILABLE');
    for (const raw of [
      'not json',
      JSON.stringify({ ...JSON.parse(configRaw), privateBaseUrl: 'http://127.0.0.1:3100' }),
      JSON.stringify({ ...JSON.parse(configRaw), privateBaseUrl: 'http://localhost:44995' }),
      JSON.stringify({ ...JSON.parse(configRaw), holdings: [] }),
      JSON.stringify({ ...JSON.parse(configRaw), holdings: [holding(), holding()] }),
      JSON.stringify({ ...JSON.parse(configRaw), holdings: [holding({ origin: 'http://acme.example.com' })] }),
      JSON.stringify({ ...JSON.parse(configRaw), timeoutMs: 249 }),
    ]) {
      fails(() => nativeWorkflowLane32Configuration(raw), 'WORKFLOW_FAILED_METRICS_READ_CONFIG_INVALID');
    }
  });

  it('selects each Holding and forwards only its registered host and origin', async () => {
    const cfg = nativeWorkflowLane32Configuration(configRaw);
    expect(selectHolding(cfg, { holdingId: 'h001', holdingSlug: 'acme' }).holdingSlug).toBe('acme');
    expect(selectHolding(cfg, { holdingId: 'h002', holdingSlug: 'northstar' }).holdingSlug).toBe('northstar');
    expect(selectHolding(cfg, { origin: 'https://northstar.example.com' }).holdingId).toBe('h002');
    // No positional fallback and no partial WorkContext.
    for (const selector of [
      undefined,
      {},
      { holdingId: 'h002' },
      { holdingSlug: 'acme' },
      { holdingId: 'h001', holdingSlug: 'northstar' },
      { origin: 'https://unknown.example.com' },
    ]) {
      fails(
        () => selectHolding(cfg, selector),
        'WORKFLOW_FAILED_METRICS_AUTHORITY_INVALID',
      );
    }

    const transport = vi.fn(
      (_url: URL, _headers: Record<string, string>, _timeoutMs: number) => ok({ count: 3 }),
    );
    const read = createFailedMetricsClient(cfg, transport);
    expect(await read({ holdingId: 'h002', holdingSlug: 'northstar' })).toEqual({ count: 3 });
    expect(transport).toHaveBeenCalledTimes(1);
    const [url, headers, timeoutMs] = transport.mock.calls[0]!;
    expect(String(url)).toBe('http://127.0.0.1:44995/api/v1/holdings/northstar/workflow/inbox-failed-metrics');
    expect(headers).toEqual({
      host: 'northstar.example.com',
      origin: 'https://northstar.example.com',
      accept: 'application/json',
      'cache-control': 'no-cache',
    });
    expect(timeoutMs).toBe(1000);
  });

  it('fails closed with no WorkContext, and on unavailable or widened replies', async () => {
    const cfg = nativeWorkflowLane32Configuration(configRaw);
    const scope = { holdingId: 'h001', holdingSlug: 'acme' };
    const transport = vi.fn(() => ok({ count: 3 }));
    await expect(createFailedMetricsClient(cfg, transport)())
      .rejects.toMatchObject({ status: 503, code: 'WORKFLOW_FAILED_METRICS_AUTHORITY_INVALID' });
    expect(transport).not.toHaveBeenCalled();
    await expect(
      createFailedMetricsClient(cfg, () => Promise.reject(new Error('down')))(scope),
    ).rejects.toMatchObject({ status: 503, code: 'WORKFLOW_FAILED_METRICS_READ_UNAVAILABLE' });
    await expect(
      createFailedMetricsClient(cfg, () => Promise.resolve({ status: 503, value: {} }))(scope),
    ).rejects.toMatchObject({ status: 503 });
    for (const value of [{}, { count: '3' }, { count: -1 }, { count: 3, extra: true }]) {
      fails(() => readFailedMetricsReply(value), 'WORKFLOW_FAILED_METRICS_READ_RESPONSE_INVALID');
    }
  });
});

describe('native lane-32 SQL/result oracle and TS cutover', () => {
  it('pins the retired SQL, the executed SQL and the result shape hashes', () => {
    expect(sha(SOURCE_SQL)).toBe('a52f4892742a611e26e2ee0d4247bbba65e48e00a9f726485114eea6f104976f');
    expect(sha(EXECUTED_SQL)).toBe('4a255c8faf3a8c8a049719a6818ff7f0e5041e12ed6722cfb664337572d21329');
    expect(sha(JSON.stringify({ count: 3 }))).toBe('0c07187ea6d064441225b3cba26a7b1e8bc702fcf332b457dae8e26892ba68a6');
    expect(EXECUTED_SQL).toContain('dead_letter_at IS NOT NULL');
    expect(EXECUTED_SQL).not.toContain('?');
  });

  it('removes the retired SQL from the switched owner adapter', async () => {
    const source = await readFile(
      new URL('../../../modules/workflow/server/src/adapters/collaboration-inbox-sources.ts', import.meta.url),
      'utf8',
    );
    expect(source).not.toContain(SOURCE_SQL);
    expect(source).not.toContain('outbox_events');
    expect(source).not.toContain('.prepare(');
    expect(source).toContain("from './native-lane-32/client'");
  });

  it('routes the live read through the native client with no local SQL', async () => {
    const read = vi.fn(async () => ({ count: 3 }));
    vi.resetModules();
    vi.doMock('../../../modules/workflow/server/src/adapters/native-lane-32/client', () => ({
      nativeInboxFailedMetrics: read,
    }));
    try {
      const { collaborationInboxSources } = await import(
        '../../../modules/workflow/server/src/adapters/collaboration-inbox-sources'
      );
      const prepare = vi.fn();
      const db = { prepare } as never;
      expect(await collaborationInboxSources(db).failedMetrics()).toEqual({ count: 3 });
      expect(read).toHaveBeenCalledWith(db, undefined);
      const scope = { holdingId: 'h002', holdingSlug: 'northstar' };
      expect(await collaborationInboxSources(db, scope).failedMetrics()).toEqual({ count: 3 });
      expect(read).toHaveBeenLastCalledWith(db, scope);
      expect(prepare).not.toHaveBeenCalled();
    } finally {
      vi.doUnmock('../../../modules/workflow/server/src/adapters/native-lane-32/client');
      vi.resetModules();
    }
  });
});
