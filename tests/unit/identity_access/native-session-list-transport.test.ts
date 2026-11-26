import { describe, it, expect, vi, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  NATIVE_SESSION_LIST_ENV,
  createNativeSessionListClient,
  nativeSessionListConfigured,
  nativeSessionListConfiguration,
  type NativeSessionListTransport,
} from '../../../packages/server-identity_access/src/public/native-session-list';

const raw = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:44996',
  timeoutMs: 1000,
  holdings: [
    { origin: 'https://acme.jesadakorn.com', holdingSlug: 'acme', holdingId: 'h001' },
    { origin: 'https://northstar.jesadakorn.com', holdingSlug: 'northstar', holdingId: 'h002' },
  ],
});
const actor = { userId: 'user-1', holdingId: 'h001', companyId: 'company-1' };
const incoming = (host = 'acme.jesadakorn.com') => new Headers({ host });
const row = {
  id: 'session-1',
  createdAt: '2026-01-01T00:00:00.000Z',
  expiresAt: '2026-01-08T00:00:00.000Z',
  userAgent: 'ERPX/1.0',
};
const ok = (value: unknown) =>
  new Response(JSON.stringify(value), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });
const client = (transport: NativeSessionListTransport) =>
  createNativeSessionListClient(nativeSessionListConfiguration(raw), transport);
const codeOf = async (run: () => Promise<unknown>) => {
  try {
    await run();
    return undefined;
  } catch (error) {
    return (error as { code?: string }).code;
  }
};

afterEach(() => {
  delete process.env[NATIVE_SESSION_LIST_ENV];
  vi.unstubAllEnvs();
});

describe('native IAM session-list transport', () => {
  it('posts an authority-free body with the exact Host and injected user', async () => {
    const transport = vi.fn().mockResolvedValue(ok({ data: [row] }));
    await expect(client(transport).read(actor, incoming())).resolves.toEqual([row]);
    const [method, url, headers, body] = transport.mock.calls[0];
    expect(method).toBe('POST');
    expect(url.toString()).toBe(
      'http://127.0.0.1:44996/api/v1/holdings/acme/identity-access/session-list-read',
    );
    expect(headers.get('host')).toBe('acme.jesadakorn.com');
    expect(headers.get('x-erpx-context-user')).toBe('user-1');
    expect(body).toBe('{}');
    expect(headers.get('idempotency-key')).toBeNull();
  });

  it('resolves a holding from the actor outside a request scope', async () => {
    const transport = vi.fn().mockResolvedValue(ok({ data: [row] }));
    await expect(client(transport).read(actor)).resolves.toEqual([row]);
    expect(transport.mock.calls[0][1].pathname).toContain('/holdings/acme/');
  });

  it('fails closed on unknown hosts, cross-holding actors and unregistered ids', async () => {
    const transport = vi.fn().mockResolvedValue(ok({ data: [row] }));
    expect(await codeOf(() => client(transport).read(actor, incoming('evil.example.com')))).toBe(
      'NATIVE_IAM_SESSION_LIST_TARGET_INVALID',
    );
    expect(await codeOf(() => client(transport).read({ ...actor, holdingId: 'h002' }, incoming()))).toBe(
      'NATIVE_IAM_SESSION_LIST_TARGET_INVALID',
    );
    expect(await codeOf(() => client(transport).read({ ...actor, holdingId: null }))).toBe(
      'NATIVE_IAM_SESSION_LIST_TARGET_INVALID',
    );
    expect(await codeOf(() => client(transport).read({ ...actor, userId: 'not a user' }))).toBe(
      'NATIVE_IAM_SESSION_LIST_REQUEST_INVALID',
    );
    expect(transport).not.toHaveBeenCalled();
  });

  it('fails closed on transport, status and payload faults', async () => {
    const fails = [
      vi.fn().mockRejectedValue(new Error('offline')),
      vi.fn().mockResolvedValue(new Response('{}', { status: 503 })),
      vi.fn().mockResolvedValue(new Response(JSON.stringify({ data: [row] }), { status: 200 })),
      vi.fn().mockResolvedValue(ok({ data: [{ ...row, userId: 'someone-else' }] })),
      vi.fn().mockResolvedValue(ok({ data: [row, row] })),
      vi.fn().mockResolvedValue(ok({ data: [{ ...row, userAgent: 3 }] })),
      vi.fn().mockResolvedValue(ok({ data: [row], extra: true })),
    ];
    for (const transport of fails)
      expect(await codeOf(() => client(transport).read(actor, incoming()))).toMatch(
        /^NATIVE_IAM_SESSION_LIST_(UNAVAILABLE|RESPONSE_INVALID)$/,
      );
  });

  it('fails closed without configuration and on malformed or reserved config', () => {
    expect(nativeSessionListConfigured()).toBe(false);
    process.env[NATIVE_SESSION_LIST_ENV] = raw;
    expect(nativeSessionListConfigured()).toBe(true);
    const bad = (value: Record<string, unknown>) =>
      expect(() => nativeSessionListConfiguration(JSON.stringify(value))).toThrowError(
        expect.objectContaining({ code: 'NATIVE_IAM_SESSION_LIST_CONFIGURATION_INVALID' }),
      );
    bad({ ...JSON.parse(raw), privateBaseUrl: 'http://127.0.0.1:3100' });
    bad({ ...JSON.parse(raw), privateBaseUrl: 'http://127.0.0.1:3201' });
    bad({ ...JSON.parse(raw), privateBaseUrl: 'https://127.0.0.1:44996' });
    bad({
      ...JSON.parse(raw),
      holdings: [{ origin: 'https://acme.jesadakorn.com', holdingSlug: 'acme', holdingId: 'h001' }, { origin: 'https://acme.jesadakorn.com', holdingSlug: 'other', holdingId: 'h002' }],
    });
    expect(() => nativeSessionListConfiguration('{not-json')).toThrowError(
      expect.objectContaining({ code: 'NATIVE_IAM_SESSION_LIST_CONFIGURATION_INVALID' }),
    );
  });
});
