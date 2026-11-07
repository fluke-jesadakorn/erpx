import { afterEach, describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  createNativeCompanyPackagesClient,
  nativeCompanyPackagesConfigured,
  nativeCompanyPackagesConfiguration,
  nativeCompanyPackagesFailure,
  NativeCompanyPackagesError,
  type NativeCompanyPackageActor,
} from '../../../packages/server-commercial/src/adapters/native-company-packages';

const actor: NativeCompanyPackageActor = {
  userId: 'operator',
  platformAssignmentId: 'platform-assignment',
  platformRole: 'platform_admin',
  roleRevision: 7,
  modules: ['provider'],
  permissions: ['platform.commercial.read'],
};
const config = (overrides: Record<string, unknown> = {}) =>
  JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44991',
    timeoutMs: 1000,
    holdings: [
      { origin: 'https://acme.example.test', holdingSlug: 'acme', holdingId: 'h001' },
    ],
    ...overrides,
  });
const incoming = (overrides: Record<string, string> = {}) =>
  new Headers({
    host: 'acme.example.test',
    cookie: '__Host-erpx-product=session',
    origin: 'https://acme.example.test',
    ...overrides,
  });
const packages = [
  {
    id: 'service',
    version: 1,
    productId: 'service',
    kind: 'base',
    visibleCapabilities: ['service.orders'],
    includedCapabilities: [],
    dependencies: [],
  },
];
const grants = [
  {
    companyId: 'company',
    packageId: 'service',
    packageVersion: 1,
    state: 'active',
    startsAt: 0,
    endsAt: null,
    version: 1,
    reason: 'Reviewed package change',
    evidence: 'Reviewed owner evidence',
  },
];
const history = [
  {
    companyId: 'company',
    packageId: 'service',
    packageVersion: 1,
    sourceGrantVersion: 1,
    sourceEventId: 'event',
    retainedAt: 1000,
  },
];
const context = {
  userId: actor.userId,
  holdingId: 'h001',
  holdingSlug: 'acme',
  companyId: 'company',
  platformAssignmentId: actor.platformAssignmentId,
  platformRole: actor.platformRole,
  roleRevision: actor.roleRevision,
  modules: [...actor.modules],
  permissions: [...actor.permissions],
  expiresAt: 9_007_199_254_740_991,
};
const overview = {
  companyId: 'company',
  revision: 'a'.repeat(64),
  packages,
  billingMode: 'sandbox',
  observedAt: 1000,
  grants,
  history,
};
const ok = (value: unknown) =>
  new Response(JSON.stringify(value), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });

afterEach(() => {
  delete process.env.ERPX_NATIVE_COMMERCIAL_CONFIG;
});

describe('native Commercial package transport', () => {
  it('rejects reserved, non-loopback, duplicate and non-HTTPS targets', () => {
    for (const port of [3000, 3001, 3100, 3101, 3103, 5432, 8080, 3200]) {
      expect(() =>
        nativeCompanyPackagesConfiguration(
          config({ privateBaseUrl: `http://127.0.0.1:${port}` }),
        ),
      ).toThrow(NativeCompanyPackagesError);
    }
    expect(() =>
      nativeCompanyPackagesConfiguration(
        config({ privateBaseUrl: 'http://192.168.1.1:44991' }),
      ),
    ).toThrow(NativeCompanyPackagesError);
    expect(() =>
      nativeCompanyPackagesConfiguration(
        config({
          holdings: [
            { origin: 'https://acme.example.test', holdingSlug: 'acme', holdingId: 'h001' },
            { origin: 'https://acme.example.test', holdingSlug: 'acme', holdingId: 'h001' },
          ],
        }),
      ),
    ).toThrow(NativeCompanyPackagesError);
    expect(() =>
      nativeCompanyPackagesConfiguration(
        config({
          holdings: [
            { origin: 'http://acme.example.test', holdingSlug: 'acme', holdingId: 'h001' },
          ],
        }),
      ),
    ).toThrow(NativeCompanyPackagesError);
    expect(() =>
      nativeCompanyPackagesConfiguration(config({ timeoutMs: 249 })),
    ).toThrow(NativeCompanyPackagesError);
  });

  it('forwards the current session, Provider assignment and exact Holding target', async () => {
    const transport = vi.fn().mockResolvedValue(ok({ data: overview, context }));
    const client = createNativeCompanyPackagesClient(
      JSON.parse(config()),
      transport as never,
    );
    const result = await client(incoming(), actor, 'h001', 'company');
    expect(result).toMatchObject({ companyId: 'company', billingMode: 'sandbox' });
    const [method, url, headers] = transport.mock.calls[0]!;
    expect(method).toBe('GET');
    expect(url.toString()).toBe(
      'http://127.0.0.1:44991/api/v1/holdings/acme/commercial/companies/company/packages',
    );
    expect(headers.get('host')).toBe('acme.example.test');
    expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
    expect(headers.get('origin')).toBe('https://acme.example.test');
    expect(headers.get('x-erpx-platform-assignment')).toBe('platform-assignment');
    expect(headers.get('x-erpx-platform-role')).toBe('platform_admin');
    expect(headers.get('x-erpx-context-company')).toBe('company');
    expect(headers.get('x-erpx-context-revision')).toBe('7');
  });

  it('rejects foreign, duplicate, over-bounded and malformed owner replies', async () => {
    const send = (data: unknown, current: unknown = context) =>
      vi.fn().mockResolvedValue(ok({ data, context: current }));
    const cases: unknown[] = [
      { ...overview, companyId: 'foreign' },
      { ...overview, revision: 'A'.repeat(64) },
      { ...overview, packages: [...packages, packages[0]] },
      { ...overview, grants: [{ ...grants[0], companyId: 'foreign' }] },
      { ...overview, grants: [...grants, { ...grants[0] }] },
      { ...overview, history: [{ ...history[0], companyId: 'foreign' }] },
      { ...overview, packages: [] },
      { ...overview, unexpected: true },
    ];
    for (const data of cases) {
      const client = createNativeCompanyPackagesClient(JSON.parse(config()), send(data) as never);
      await expect(client(incoming(), actor, 'h001', 'company')).rejects.toThrow(
        NativeCompanyPackagesError,
      );
    }
    const client = createNativeCompanyPackagesClient(
      JSON.parse(config()),
      send(overview, { ...context, userId: 'other' }) as never,
    );
    await expect(client(incoming(), actor, 'h001', 'company')).rejects.toMatchObject({
      code: 'NATIVE_COMMERCIAL_CONTEXT_CHANGED',
    });
  });

  it('fails closed for the wrong Host, unknown Holding and unavailable service', async () => {
    const transport = vi.fn().mockResolvedValue(ok({ data: overview, context }));
    const client = createNativeCompanyPackagesClient(
      JSON.parse(config()),
      transport as never,
    );
    await expect(
      client(incoming({ host: 'northstar.example.test' }), actor, 'h001', 'company'),
    ).rejects.toMatchObject({ code: 'NATIVE_COMMERCIAL_AUTHORITY_INVALID' });
    await expect(
      client(incoming(), actor, 'h002', 'company'),
    ).rejects.toMatchObject({ code: 'NATIVE_COMMERCIAL_AUTHORITY_INVALID' });
    expect(nativeCompanyPackagesConfigured()).toBe(false);
    const missing = nativeCompanyPackagesConfiguration(config());
    expect(missing.holdings[0]!.holdingId).toBe('h001');
  });

  it('maps bounded owner failures to explicit API errors', async () => {
    const cases: [number, string, number, string][] = [
      [503, 'NATIVE_COMMERCIAL_UNAVAILABLE', 503, 'NATIVE_COMMERCIAL_UNAVAILABLE'],
      [409, 'COMMERCIAL_PURCHASE_CHANGED', 409, 'COMMERCIAL_PURCHASE_CHANGED'],
      [422, 'COMMERCIAL_REQUEST_INVALID', 422, 'COMMERCIAL_REQUEST_INVALID'],
      [401, 'UNAUTHENTICATED', 401, 'UNAUTHENTICATED'],
      [404, 'NOT_FOUND', 404, 'NOT_FOUND'],
    ];
    for (const [status, code, expectedStatus, expectedCode] of cases) {
      const transport = vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ error: { code, message: 'Synthetic owner failure.' } }), {
          status,
          headers: { 'content-type': 'application/json' },
        }),
      );
      const client = createNativeCompanyPackagesClient(
        JSON.parse(config()),
        transport as never,
      );
      try {
        await client(incoming(), actor, 'h001', 'company');
        expect.unreachable();
      } catch (error) {
        expect(error).toBeInstanceOf(NativeCompanyPackagesError);
        const failure = nativeCompanyPackagesFailure(error);
        expect(failure).toMatchObject({ status: expectedStatus, code: expectedCode });
      }
    }
    expect(
      nativeCompanyPackagesFailure(
        new NativeCompanyPackagesError('NATIVE_COMMERCIAL_CONFIGURATION_INVALID'),
      ),
    ).toMatchObject({ status: 503, code: 'NATIVE_COMMERCIAL_UNAVAILABLE' });
    expect(nativeCompanyPackagesFailure(new Error('unknown'))).toBeUndefined();
  });
});
