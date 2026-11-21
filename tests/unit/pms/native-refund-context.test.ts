import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  refundRequestContext,
  REFUND_CONTEXT_CONFIG_KEY,
} from '../../../packages/server-pms/src/adapters/native-refund-context';

const config = (overrides: Record<string, unknown> = {}) =>
  JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44994',
    timeoutMs: 1000,
    holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }],
    ...overrides,
  });

const incoming = (overrides: Record<string, string> = {}) =>
  new Headers({
    host: 'acme.example.com',
    cookie: '__Host-erpx-session=session',
    ...overrides,
  });

const contextRow = {
  id: 'req-1',
  payment_id: 'pay-1',
  amount_minor: 2000,
  allocations_json: '[]',
  reason: 'Guest asked',
  requested_by: 'user-1',
  approved_by: null,
  provider_id: 're_1',
  state: 'failed',
  generation: 0,
  property_id: 'prop-1',
  account_id: 'acct-1',
  livemode: false,
};

const ok = (value: unknown) =>
  new Response(JSON.stringify(value), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });

describe('native PMS refund-context transport', () => {
  beforeEach(() => {
    process.env[REFUND_CONTEXT_CONFIG_KEY] = config();
  });
  afterEach(() => {
    delete process.env[REFUND_CONTEXT_CONFIG_KEY];
    vi.unstubAllGlobals();
  });

  it('returns the validated request context on 200', async () => {
    const fetchMock = vi.fn().mockResolvedValue(ok({ data: contextRow, context: { userId: 'u' } }));
    vi.stubGlobal('fetch', fetchMock);
    const result = await refundRequestContext(incoming(), 'h001', 'req-1');
    expect(result).toMatchObject({ id: 'req-1', property_id: 'prop-1', livemode: false });
    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toBe(
      'http://127.0.0.1:44994/api/v1/holdings/acme/pms/booking-refunds/context?request=req-1',
    );
    expect((init.headers as Record<string, string>).host).toBe('acme.example.com');
    expect((init.headers as Record<string, string>).cookie).toContain('__Host-erpx-session');
  });

  it('maps the ABSENT record code to the retained null contract', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ error: { code: 'PMS_REFUND_CONTEXT_ABSENT' } }), {
          status: 404,
          headers: { 'content-type': 'application/json' },
        }),
      ),
    );
    await expect(refundRequestContext(incoming(), 'h001', 'missing')).resolves.toBeNull();
  });

  it('fails closed without configuration', async () => {
    delete process.env[REFUND_CONTEXT_CONFIG_KEY];
    await expect(refundRequestContext(incoming(), 'h001', 'req-1')).rejects.toMatchObject({
      code: 'NATIVE_PMS_REFUND_CONTEXT_UNAVAILABLE',
    });
  });

  it('fails closed on an unknown holding', async () => {
    await expect(refundRequestContext(incoming(), 'h999', 'req-1')).rejects.toMatchObject({
      code: 'NATIVE_PMS_REFUND_CONTEXT_UNAVAILABLE',
    });
  });

  it('fails closed on a reserved privateBaseUrl port', async () => {
    for (const port of [3100, 3101, 3103, 5432, 3250]) {
      process.env[REFUND_CONTEXT_CONFIG_KEY] = config({
        privateBaseUrl: `http://127.0.0.1:${port}`,
      });
      await expect(refundRequestContext(incoming(), 'h001', 'req-1')).rejects.toMatchObject({
        code: 'NATIVE_PMS_REFUND_CONTEXT_UNAVAILABLE',
      });
    }
  });

  it('fails closed when holdings are missing or malformed', async () => {
    process.env[REFUND_CONTEXT_CONFIG_KEY] = config({ holdings: [] });
    await expect(refundRequestContext(incoming(), 'h001', 'req-1')).rejects.toMatchObject({
      code: 'NATIVE_PMS_REFUND_CONTEXT_UNAVAILABLE',
    });
    process.env[REFUND_CONTEXT_CONFIG_KEY] = config({
      holdings: [{ origin: 'http://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }],
    });
    await expect(refundRequestContext(incoming(), 'h001', 'req-1')).rejects.toMatchObject({
      code: 'NATIVE_PMS_REFUND_CONTEXT_UNAVAILABLE',
    });
  });

  it('maps only the ABSENT record code to null; other 404s fail closed', async () => {
    const absent = new Response(JSON.stringify({ error: { code: 'PMS_REFUND_CONTEXT_ABSENT' } }), {
      status: 404,
      headers: { 'content-type': 'application/json' },
    });
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(absent));
    await expect(refundRequestContext(incoming(), 'h001', 'missing')).resolves.toBeNull();

    const unknownTarget = new Response(JSON.stringify({ error: { code: 'NOT_FOUND' } }), {
      status: 404,
      headers: { 'content-type': 'application/json' },
    });
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(unknownTarget));
    await expect(refundRequestContext(incoming(), 'h001', 'req-1')).rejects.toMatchObject({
      status: 503,
    });
  });
  it('fails closed when the native payload drops a required field', async () => {
    const { livemode, ...partial } = contextRow;
    void livemode;
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(ok({ data: partial, context: {} })));
    await expect(refundRequestContext(incoming(), 'h001', 'req-1')).rejects.toMatchObject({
      status: 502,
    });
  });

  it('rejects an invalid request id before any fetch', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    await expect(refundRequestContext(incoming(), 'h001', '')).rejects.toMatchObject({
      status: 422,
    });
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
