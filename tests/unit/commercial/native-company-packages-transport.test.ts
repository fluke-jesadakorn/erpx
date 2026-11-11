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
import {
  createNativePackageGrantsClient,
  nativePackageGrantsConfigured,
  nativePackageGrantsConfiguration,
  nativePackageGrantsFailure,
  NativePackageGrantsError,
  type NativePackageGrantActor,
} from '../../../packages/feature-platform_administration/src/adapters/server/native-platform-administration';

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

const grantActor: NativePackageGrantActor = {
  userId: 'operator',
  platformAssignmentId: 'platform-assignment',
  platformRole: 'platform_admin',
  roleRevision: 7,
  modules: ['commercial'],
  permissions: ['platform.commercial.manage'],
};
const grantConfig = (overrides: Record<string, unknown> = {}) =>
  JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44991',
    timeoutMs: 1000,
    holdings: [
      { origin: 'https://acme.example.test', holdingSlug: 'acme', holdingId: 'h001' },
    ],
    ...overrides,
  });
const grantIncoming = (overrides: Record<string, string> = {}) =>
  new Headers({
    host: 'acme.example.test',
    cookie: '__Host-erpx-product=session',
    origin: 'https://acme.example.test',
    ...overrides,
  });
const grantBody = () =>
  JSON.stringify({
    holdingId: 'h001',
    request: {
      companyId: 'company',
      expectedRevision: 'a'.repeat(64),
      idempotencyKey: 'package-grant-command-0001',
      changes: [
        {
          expectedVersion: 1,
          grant: {
            companyId: 'company',
            packageId: 'legal',
            packageVersion: 1,
            state: 'active',
            startsAt: 0,
            endsAt: null,
            version: 2,
            reason: 'Reviewed change',
            evidence: 'Reviewed owner evidence',
          },
        },
      ],
    },
  });
const grantReview = {
  comparison: {
    companyId: 'company',
    expectedRevision: 'a'.repeat(64),
    changes: [
      {
        expectedVersion: 1,
        grant: {
          companyId: 'company',
          packageId: 'legal',
          packageVersion: 1,
          state: 'active',
          startsAt: 0,
          endsAt: null,
          version: 2,
          reason: 'Reviewed change',
          evidence: 'Reviewed owner evidence',
        },
      },
    ],
    before: [],
    after: [],
  },
  request: JSON.parse(grantBody()).request,
  review: { token: 'review-token', expiresAt: 1000 },
};
const grantContext = {
  userId: grantActor.userId,
  holdingId: 'h001',
  holdingSlug: 'acme',
  platformAssignmentId: grantActor.platformAssignmentId,
  platformRole: grantActor.platformRole,
  roleRevision: grantActor.roleRevision,
};
const grantOk = (value: unknown) =>
  new Response(JSON.stringify(value), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });

describe('native Commercial package grant transport', () => {
  afterEach(() => {
    delete process.env.ERPX_NATIVE_COMMERCIAL_CONFIG;
    vi.unstubAllEnvs();
  });

  it('is selected only by the established native Commercial configuration name', () => {
    delete process.env.ERPX_NATIVE_COMMERCIAL_CONFIG;
    expect(nativePackageGrantsConfigured()).toBe(false);
    vi.stubEnv('ERPX_NATIVE_COMMERCIAL_CONFIG', grantConfig());
    expect(nativePackageGrantsConfigured()).toBe(true);
    expect(nativePackageGrantsConfiguration(grantConfig()).holdings[0]!.holdingSlug).toBe('acme');
    expect(() => nativePackageGrantsConfiguration('{')).toThrow(NativePackageGrantsError);
    expect(() =>
      nativePackageGrantsConfiguration(
        grantConfig({ privateBaseUrl: 'http://127.0.0.1:3100' }),
      ),
    ).toThrow(NativePackageGrantsError);
  });

  it('forwards the exact host, origin, cookie, assignment and body without minting authority', async () => {
    let seen:
      | { method: string; url: URL; headers: Headers; body: string | undefined }
      | undefined;
    const transport = vi.fn(async (method: string, url: URL, headers: Headers, body: string) => {
      seen = { method, url, headers, body };
      return grantOk({ data: grantReview, context: grantContext });
    });
    const client = createNativePackageGrantsClient(JSON.parse(grantConfig()), transport as never);
    const data = await client.preview(grantIncoming(), grantActor, grantBody());
    expect(data).toEqual(grantReview);
    expect(seen?.method).toBe('POST');
    expect(seen?.url.pathname).toBe(
      '/api/v1/platform/commercial/package-grants/batches/preview',
    );
    expect(seen?.headers.get('host')).toBe('acme.example.test');
    expect(seen?.headers.get('origin')).toBe('https://acme.example.test');
    expect(seen?.headers.get('cookie')).toBe('__Host-erpx-product=session');
    expect(seen?.headers.get('x-erpx-platform-assignment')).toBe('platform-assignment');
    expect(seen?.headers.get('x-erpx-platform-role')).toBe('platform_admin');
    expect(seen?.headers.get('x-erpx-context-revision')).toBe('7');
    expect(seen?.headers.get('idempotency-key')).toBeNull();
    expect(seen?.body).toBe(grantBody());
  });

  it('requires and forwards the reviewed Idempotency-Key for apply', async () => {
    let seen: Headers | undefined;
    const transport = vi.fn(async (_method: string, _url: URL, headers: Headers) => {
      seen = headers;
      return grantOk({ data: { receipt: grantReview.comparison }, context: grantContext });
    });
    const client = createNativePackageGrantsClient(JSON.parse(grantConfig()), transport as never);
    const data = await client.apply(
      grantIncoming(),
      grantActor,
      grantBody(),
      'package-grant-command-0001',
    );
    expect(data).toEqual({ receipt: grantReview.comparison });
    expect(seen?.get('idempotency-key')).toBe('package-grant-command-0001');
    await expect(
      client.apply(grantIncoming(), grantActor, grantBody(), 'short'),
    ).rejects.toMatchObject({ code: 'NATIVE_COMMERCIAL_REQUEST_INVALID' });
  });

  it('rejects a foreign origin and a context that does not bind the current authority', async () => {
    const transport = vi.fn().mockResolvedValue(
      grantOk({
        data: grantReview,
        context: { ...grantContext, platformAssignmentId: 'another-assignment' },
      }),
    );
    const client = createNativePackageGrantsClient(JSON.parse(grantConfig()), transport as never);
    await expect(
      client.preview(
        grantIncoming({ origin: 'https://evil.example.test' }),
        grantActor,
        grantBody(),
      ),
    ).rejects.toMatchObject({ code: 'ORIGIN_FORBIDDEN', status: 403 });
    await expect(client.preview(grantIncoming(), grantActor, grantBody())).rejects.toMatchObject({
      code: 'NATIVE_COMMERCIAL_CONTEXT_CHANGED',
    });
  });

  it('surfaces a configured owner failure instead of falling back to TypeScript', async () => {
    for (const [status, code, expectedStatus, expectedCode] of [
      [403, 'FRESH_MFA_REQUIRED', 403, 'FRESH_MFA_REQUIRED'],
      [409, 'PACKAGE_REVIEW_CHANGED', 409, 'PACKAGE_REVIEW_CHANGED'],
      [422, 'VALIDATION_FAILED', 422, 'VALIDATION_FAILED'],
      [401, 'UNAUTHENTICATED', 401, 'UNAUTHENTICATED'],
      [503, 'COMMERCIAL_UNAVAILABLE', 503, 'NATIVE_COMMERCIAL_UNAVAILABLE'],
    ] as const) {
      const transport = vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ error: { code, message: 'Synthetic owner failure.' } }), {
          status,
          headers: { 'content-type': 'application/json' },
        }),
      );
      const client = createNativePackageGrantsClient(JSON.parse(grantConfig()), transport as never);
      try {
        await client.preview(grantIncoming(), grantActor, grantBody());
        expect.unreachable();
      } catch (error) {
        expect(error).toBeInstanceOf(NativePackageGrantsError);
        expect(nativePackageGrantsFailure(error)).toMatchObject({
          status: expectedStatus,
          code: expectedCode,
        });
      }
    }
    expect(
      nativePackageGrantsFailure(
        new NativePackageGrantsError('NATIVE_COMMERCIAL_CONFIGURATION_INVALID'),
      ),
    ).toMatchObject({ status: 503, code: 'NATIVE_COMMERCIAL_UNAVAILABLE' });
    expect(nativePackageGrantsFailure(new Error('unknown'))).toBeUndefined();
  });
});
