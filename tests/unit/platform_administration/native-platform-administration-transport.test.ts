import { describe, it, expect, vi } from 'vitest';
import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
vi.mock('server-only', () => ({}));
import {
  createNativePlatformAdminClient,
  nativePlatformAdminConfiguration,
  nativePlatformAdminFailure,
  NativePlatformAdminError,
  type NativePlatformAdminActor,
} from '../../../packages/feature-platform_administration/src/adapters/server/native-platform-administration';
import { platformAdminActor, nativePlatformAdminAvailable, nativePlatformAdministrationOrRetained, nativePlatformMigrate, nativePlatformOperationsForPresentation } from '../../../packages/feature-platform_administration/src/lib/native-platform-administration';
import {
  createNativePlatformExecutiveClient,
  nativePlatformExecutiveConfiguration,
  nativePlatformExecutiveFailure,
  nativePlatformExecutiveOrRetained,
  nativePlatformExecutiveSnapshot,
  NativePlatformExecutiveError,
  type NativePlatformExecutiveActor,
} from '../../../packages/feature-admin/src/public/native-platform-executive';

const config = (overrides: Record<string, unknown> = {}) =>
  JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44991',
    timeoutMs: 1000,
    holdings: [
      { origin: 'https://erpx-internal.example.test', holdingSlug: 'erpx', holdingId: 'h003' },
    ],
    ...overrides,
  });
const incoming = (overrides: Record<string, string> = {}) =>
  new Headers({
    host: 'erpx-internal.example.test',
    cookie: '__Host-erpx-product=session',
    origin: 'https://erpx-internal.example.test',
    ...overrides,
  });
const work = {
  userId: 'operator',
  holdingId: 'h003',
  holdingSlug: 'erpx',
  companyId: 'erpx',
  branchId: null,
  departmentId: null,
  assignmentId: 'work-assignment',
  authority: { revision: 7, assignmentVersion: 3 },
  modules: ['manage_erpx'],
  iam: { permissions: ['manage_erpx.access'] },
};
const platform = { userId: 'operator', assignmentId: 'platform-assignment', role: 'platform_admin' };
const actor: NativePlatformAdminActor = platformAdminActor(work, platform);
const context = {
  userId: 'operator',
  holdingId: 'h003',
  holdingSlug: 'erpx',
  companyId: 'erpx',
  branchId: null,
  departmentId: null,
  workAssignmentId: 'work-assignment',
  platformAssignmentId: 'platform-assignment',
  platformRole: 'platform_admin',
  assignmentVersion: 3,
  revision: 7,
  modules: ['manage_erpx'],
  permissions: ['manage_erpx.access'],
  expiresAt: 9_007_199_254_740_991,
};
const slot = { id: 'h004', write: 'H004_DB', read: 'H004_READ_DB', files: 'H004_FILES' };
const row = {
  id: 'holding-provisioning-1',
  holdingId: 'h004',
  slug: 'northstar',
  displayName: 'Northstar',
  writeBinding: 'H004_DB',
  readBinding: 'H004_READ_DB',
  filesBinding: 'H004_FILES',
  status: 'active',
  requestedBy: 'operator',
  reason: 'Reviewed onboarding and verified health.',
  error: null,
  version: 2,
  requestedAt: 1,
  activatedAt: 2,
  updatedAt: 2,
};
const snapshot = {
  provisioning: { rows: [row], availableSlots: [slot] },
  operations: {
    generatedAt: 10,
    actorRole: 'platform_admin',
    controlHealth: [{ service: 'jobs', heartbeatAt: 9, details: '{}' }],
    holdings: [
      {
        id: 'h001',
        slug: 'acme',
        displayName: 'Acme',
        status: 'attention',
        pendingEvents: 1,
        deadLetterEvents: 1,
        claimedEvents: 0,
        oldestPendingAt: 3,
        jobHeartbeatAt: 9,
        error: null,
        deadLetters: [
          {
            id: 'event',
            eventType: 'test.operation',
            aggregateId: 'aggregate',
            aggregateVersion: 1,
            attempts: 6,
            lastError: 'PROJECTOR_TIMEOUT',
            deadLetterAt: 4,
          },
        ],
        migrations: {
          track: 'holding-write',
          applied: 10,
          pending: [],
          latestApplied: '0010',
          error: null,
        },
      },
    ],
    totals: { holdings: 1, pendingEvents: 1, deadLetterEvents: 1, claimedEvents: 0 },
  },
};
const ok = (value: unknown) =>
  new Response(JSON.stringify(value), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });
const executiveSnapshotData = {
  scope: 'platform',
  referenceAt: 10,
  filters: { from: '2026-06-01', to: '2026-06-30' },
  sourceRevision: 'a'.repeat(64),
  metrics: [
    {
      id: 'failed-provisioning',
      label: 'Failed provisioning jobs',
      formula: 'Count failed provisioning events.',
      dateBasis: 'Failure event time (UTC)',
      total: '1',
      rows: [{ label: '2026-06', value: '1' }],
      status: 'ready',
      sourceReferences: [
        {
          dataset: 'platform_holding_provisioning_events',
          revision: 'a'.repeat(64),
          referencedAt: 10,
        },
      ],
    },
    {
      id: 'abnormal-platform-services',
      label: 'Abnormal platform services',
      formula: 'Count abnormal health observations.',
      dateBasis: 'Latest health observation',
      total: '0',
      rows: [],
      status: 'empty',
      sourceReferences: [
        { dataset: 'platform_health', revision: 'a'.repeat(64), referencedAt: 10 },
      ],
    },
  ],
};
const executiveSourcesData = {
  id: '2026-06',
  name: '2026-06',
  metric: 'failed-provisioning',
  filters: { from: '2026-06-01', to: '2026-06-30' },
  rows: [{ created_at: 1, action: 'failed' }],
  page: 1,
  hasNext: false,
};

describe('native Platform Administration transport', () => {
  it('rejects reserved, non-loopback and duplicate targets', () => {
    expect(() =>
      nativePlatformAdminConfiguration(
        config({ privateBaseUrl: 'http://127.0.0.1:3101' }),
      ),
    ).toThrow(NativePlatformAdminError);
    expect(() =>
      nativePlatformAdminConfiguration(
        config({ privateBaseUrl: 'http://192.168.1.1:44991' }),
      ),
    ).toThrow(NativePlatformAdminError);
    expect(() =>
      nativePlatformAdminConfiguration(
        config({
          holdings: [
            { origin: 'https://erpx-internal.example.test', holdingSlug: 'erpx', holdingId: 'h003' },
            { origin: 'https://erpx-internal.example.test', holdingSlug: 'erpx', holdingId: 'h003' },
          ],
        }),
      ),
    ).toThrow(NativePlatformAdminError);
    expect(() =>
      nativePlatformAdminConfiguration(
        config({
          holdings: [
            { origin: 'http://erpx-internal.example.test', holdingSlug: 'erpx', holdingId: 'h003' },
          ],
        }),
      ),
    ).toThrow(NativePlatformAdminError);
  });

  it('forwards the actual session, work context and platform assignment', async () => {
    const transport = vi.fn().mockResolvedValue(ok({ data: snapshot, context }));
    const client = createNativePlatformAdminClient(JSON.parse(config()), transport as never);
    const result = await client.operations(incoming(), actor);
    expect(result.provisioning.rows[0]).toMatchObject({ slug: 'northstar', status: 'active' });
    expect(result.operations.holdings[0]).toMatchObject({ status: 'attention', pendingEvents: 1 });
    expect(result.operations.holdings[0].deadLetters[0]).toMatchObject({ eventType: 'test.operation' });
    const [method, url, headers] = transport.mock.calls[0]!;
    expect(method).toBe('GET');
    expect(url.toString()).toBe('http://127.0.0.1:44991/api/v1/platform/operations');
    expect(headers.get('host')).toBe('erpx-internal.example.test');
    expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
    expect(headers.get('x-erpx-context-company')).toBe('erpx');
    expect(headers.get('x-erpx-context-assignment')).toBe('work-assignment');
    expect(headers.get('x-erpx-context-revision')).toBe('7');
    expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
    expect(headers.get('x-erpx-platform-assignment')).toBe('platform-assignment');
  });

  it('requires the exact Platform Holding, role and current work context', () => {
    expect(() => platformAdminActor({ ...work, holdingId: 'h001' }, platform)).toThrow(
      NativePlatformAdminError,
    );
    expect(() => platformAdminActor({ ...work, holdingSlug: 'acme' }, platform)).toThrow(
      NativePlatformAdminError,
    );
    expect(() =>
      platformAdminActor(work, { ...platform, role: 'holding_owner' }),
    ).toThrow(NativePlatformAdminError);
    expect(() => platformAdminActor(work, { ...platform, userId: 'other' })).toThrow(
      NativePlatformAdminError,
    );
  });

  it('sends one keyed provisioning command with the exact origin and returns the receipt', async () => {
    const response = {
      data: {
        id: 'holding-provisioning-1',
        holdingId: 'h004',
        slug: 'northstar',
        displayName: 'Northstar',
        status: 'active',
        version: 2,
        activatedAt: 2,
      },
      context,
    };
    const transport = vi.fn().mockResolvedValue(ok(response));
    const client = createNativePlatformAdminClient(JSON.parse(config()), transport as never);
    const body = {
      holdingId: 'h004',
      slug: 'northstar',
      displayName: 'Northstar',
      reason: 'Reviewed onboarding and verified health.',
    };
    const result = await client.provision(incoming(), actor, body, 'platform-provision-key-1');
    expect(result).toEqual(response.data);
    const [method, url, headers, sent] = transport.mock.calls[0]!;
    expect(method).toBe('POST');
    expect(url.toString()).toBe('http://127.0.0.1:44991/api/v1/platform/operations/provision');
    expect(headers.get('origin')).toBe('https://erpx-internal.example.test');
    expect(headers.get('idempotency-key')).toBe('platform-provision-key-1');
    expect(headers.get('content-type')).toBe('application/json');
    expect(headers.get('x-erpx-method')).toBe('POST');
    expect(headers.get('x-erpx-request-method')).toBe('POST');
    expect(JSON.parse(sent)).toEqual(body);
  });

  it('sends a reviewed dead-letter recovery and rejects malformed receipts', async () => {
    const response = {
      data: {
        eventId: 'event',
        holdingId: 'h001',
        status: 'queued',
        previousAttempts: 6,
        previousError: 'PROJECTOR_TIMEOUT',
        recoveredAt: 10,
      },
      context,
    };
    const transport = vi.fn().mockResolvedValue(ok(response));
    const client = createNativePlatformAdminClient(JSON.parse(config()), transport as never);
    const result = await client.recover(
      incoming(),
      actor,
      {
        holdingId: 'h001',
        eventId: 'event',
        expectedAttempts: 6,
        expectedLastError: 'PROJECTOR_TIMEOUT',
        reason: 'Provider is healthy and the projection is safe to replay.',
      },
      'platform-recovery-key-1',
    );
    expect(result.status).toBe('queued');
    expect(transport.mock.calls[0]![1].toString()).toBe(
      'http://127.0.0.1:44991/api/v1/platform/operations/recover',
    );
    const invalid = createNativePlatformAdminClient(
      JSON.parse(config()),
      vi.fn().mockResolvedValue(ok({ data: { ...response.data, extra: true }, context })) as never,
    );
    await expect(
      invalid.recover(incoming(), actor, {}, 'platform-recovery-key-2'),
    ).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID' });
  });

  it('sends one keyed Holding migration command and rejects an invalid receipt', async () => {
    const response = {
      data: {
        runId: 'platform-migration-1',
        holdingId: 'h001',
        track: 'holding-write',
        status: 'succeeded',
        appliedBefore: 4,
        appliedAfter: 10,
        pending: [],
        finishedAt: 10,
      },
      context,
    };
    const transport = vi.fn().mockResolvedValue(ok(response));
    const client = createNativePlatformAdminClient(JSON.parse(config()), transport as never);
    const body = {
      holdingId: 'h001',
      track: 'holding-write',
      reason: 'Apply the reviewed release schema before enabling the next workflow.',
    };
    const result = await client.migrate(
      incoming(),
      actor,
      body,
      'platform-migration-key-1',
    );
    expect(result).toEqual(response.data);
    const [method, url, headers, sent] = transport.mock.calls[0]!;
    expect(method).toBe('POST');
    expect(url.toString()).toBe('http://127.0.0.1:44991/api/v1/platform/operations/migrate');
    expect(headers.get('origin')).toBe('https://erpx-internal.example.test');
    expect(headers.get('idempotency-key')).toBe('platform-migration-key-1');
    expect(headers.get('content-type')).toBe('application/json');
    expect(JSON.parse(sent)).toEqual(body);
    const invalid = createNativePlatformAdminClient(
      JSON.parse(config()),
      vi.fn().mockResolvedValue(
        ok({ data: { ...response.data, appliedAfter: 3 }, context }),
      ) as never,
    );
    await expect(
      invalid.migrate(incoming(), actor, body, 'platform-migration-key-2'),
    ).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID' });
    const missingContext = createNativePlatformAdminClient(
      JSON.parse(config()),
      vi.fn().mockResolvedValue(ok({ data: response.data })) as never,
    );
    await expect(
      missingContext.migrate(incoming(), actor, body, 'platform-migration-key-3'),
    ).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID' });
  });

  it('fails closed when the native Platform configuration is absent or invalid', async () => {
    const previous = process.env.ERPX_NATIVE_PLATFORM_ADMIN_CONFIG;
    const previousLong = process.env.ERPX_NATIVE_PLATFORM_ADMINISTRATION_CONFIG;
    try {
      delete process.env.ERPX_NATIVE_PLATFORM_ADMIN_CONFIG;
      delete process.env.ERPX_NATIVE_PLATFORM_ADMINISTRATION_CONFIG;
      expect(
        await nativePlatformMigrate(
          work,
          platform,
          incoming(),
          {
            holdingId: 'h001',
            track: 'holding-write',
            reason: 'Apply the reviewed release schema before enabling the next workflow.',
          },
          'platform-migration-key-4',
        ),
      ).toBeUndefined();
      process.env.ERPX_NATIVE_PLATFORM_ADMIN_CONFIG = '{}';
      await expect(
        nativePlatformMigrate(
          work,
          platform,
          incoming(),
          {
            holdingId: 'h001',
            track: 'holding-write',
            reason: 'Apply the reviewed release schema before enabling the next workflow.',
          },
          'platform-migration-key-5',
        ),
      ).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_ADMIN_CONFIGURATION_INVALID' });
    } finally {
      if (previous === undefined) delete process.env.ERPX_NATIVE_PLATFORM_ADMIN_CONFIG;
      else process.env.ERPX_NATIVE_PLATFORM_ADMIN_CONFIG = previous;
      if (previousLong === undefined)
        delete process.env.ERPX_NATIVE_PLATFORM_ADMINISTRATION_CONFIG;
      else process.env.ERPX_NATIVE_PLATFORM_ADMINISTRATION_CONFIG = previousLong;
    }
  });

  it('uses the retained path only while either configuration name is unset', async () => {
    const short = process.env.ERPX_NATIVE_PLATFORM_ADMIN_CONFIG;
    const long = process.env.ERPX_NATIVE_PLATFORM_ADMINISTRATION_CONFIG;
    try {
      delete process.env.ERPX_NATIVE_PLATFORM_ADMIN_CONFIG;
      delete process.env.ERPX_NATIVE_PLATFORM_ADMINISTRATION_CONFIG;
      expect(nativePlatformAdminAvailable()).toBe(false);
      expect(
        await nativePlatformOperationsForPresentation(work, platform, incoming()),
      ).toBeUndefined();

      process.env.ERPX_NATIVE_PLATFORM_ADMINISTRATION_CONFIG = '{}';
      expect(nativePlatformAdminAvailable()).toBe(true);
      await expect(
        nativePlatformOperationsForPresentation(work, platform, incoming()),
      ).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_ADMIN_CONFIGURATION_INVALID' });

      delete process.env.ERPX_NATIVE_PLATFORM_ADMINISTRATION_CONFIG;
      process.env.ERPX_NATIVE_PLATFORM_ADMIN_CONFIG = '{}';
      expect(nativePlatformAdminAvailable()).toBe(true);
      await expect(
        nativePlatformOperationsForPresentation(work, platform, incoming()),
      ).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_ADMIN_CONFIGURATION_INVALID' });
    } finally {
      if (short === undefined) delete process.env.ERPX_NATIVE_PLATFORM_ADMIN_CONFIG;
      else process.env.ERPX_NATIVE_PLATFORM_ADMIN_CONFIG = short;
      if (long === undefined) delete process.env.ERPX_NATIVE_PLATFORM_ADMINISTRATION_CONFIG;
      else process.env.ERPX_NATIVE_PLATFORM_ADMINISTRATION_CONFIG = long;
    }
  });

  it('never replaces a configured native failure with retained data', async () => {
    const retained = vi.fn(async () => 'retained');
    expect(
      await nativePlatformAdministrationOrRetained(async () => undefined, retained),
    ).toBe('retained');
    expect(retained).toHaveBeenCalledTimes(1);

    retained.mockClear();
    expect(
      await nativePlatformAdministrationOrRetained(async () => 'native', retained),
    ).toBe('native');
    expect(retained).not.toHaveBeenCalled();

    await expect(
      nativePlatformAdministrationOrRetained(async () => {
        throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_UNAVAILABLE');
      }, retained),
    ).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_ADMIN_UNAVAILABLE' });
    expect(retained).not.toHaveBeenCalled();
  });

  it('surfaces a configured but unavailable native service without retaining data', async () => {
    const server = createServer((_request, response) => {
      response.writeHead(503, { 'content-type': 'application/json' });
      response.end(
        JSON.stringify({ error: { code: 'SERVICE_UNAVAILABLE', message: 'offline' } }),
      );
    });
    await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
    const short = process.env.ERPX_NATIVE_PLATFORM_ADMIN_CONFIG;
    const long = process.env.ERPX_NATIVE_PLATFORM_ADMINISTRATION_CONFIG;
    try {
      delete process.env.ERPX_NATIVE_PLATFORM_ADMIN_CONFIG;
      process.env.ERPX_NATIVE_PLATFORM_ADMINISTRATION_CONFIG = config({
        privateBaseUrl: `http://127.0.0.1:${(server.address() as AddressInfo).port}`,
        timeoutMs: 250,
      });
      const retained = vi.fn(async (): Promise<never> => {
        throw new Error('retained path must not run');
      });
      await expect(
        nativePlatformAdministrationOrRetained(
          () => nativePlatformOperationsForPresentation(work, platform, incoming()),
          retained,
        ),
      ).rejects.toMatchObject({ code: 'SERVICE_UNAVAILABLE', status: 503 });
      expect(retained).not.toHaveBeenCalled();
    } finally {
      if (short === undefined) delete process.env.ERPX_NATIVE_PLATFORM_ADMIN_CONFIG;
      else process.env.ERPX_NATIVE_PLATFORM_ADMIN_CONFIG = short;
      if (long === undefined) delete process.env.ERPX_NATIVE_PLATFORM_ADMINISTRATION_CONFIG;
      else process.env.ERPX_NATIVE_PLATFORM_ADMINISTRATION_CONFIG = long;
      await new Promise<void>((resolve) => server.close(() => resolve()));
    }
  });

  it('rejects a foreign reply context and preserves a bounded owner failure', async () => {
    const foreign = createNativePlatformAdminClient(
      JSON.parse(config()),
      vi.fn().mockResolvedValue(ok({ data: snapshot, context: { ...context, companyId: 'foreign' } })) as never,
    );
    await expect(foreign.operations(incoming(), actor)).rejects.toMatchObject({
      code: 'NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID',
    });
    const denied = createNativePlatformAdminClient(
      JSON.parse(config()),
      vi.fn().mockResolvedValue(
        new Response(
          JSON.stringify({ error: { code: 'FORBIDDEN', message: 'Only a Platform administrator can perform this action.' } }),
          { status: 403, headers: { 'content-type': 'application/json' } },
        ),
      ) as never,
    );
    await expect(
      denied.provision(
        incoming(),
        actor,
        { holdingId: 'h004' },
        'platform-provision-key-2',
      ),
    ).rejects.toMatchObject({ status: 403, code: 'FORBIDDEN' });
  });

  it('reads the native operations projection and executive metrics without a retained fallback', async () => {
    const transport = vi
      .fn()
      .mockResolvedValueOnce(ok({ data: snapshot.operations, context }))
      .mockResolvedValueOnce(ok({ data: executiveSnapshotData, context }))
      .mockResolvedValueOnce(ok({ data: executiveSourcesData, context }));
    const client = createNativePlatformAdminClient(
      JSON.parse(config()),
      transport as never,
    );
    const projection = await client.operationsProjection(incoming(), actor);
    expect(projection.totals).toEqual({
      holdings: 1,
      pendingEvents: 1,
      deadLetterEvents: 1,
      claimedEvents: 0,
    });
    expect(transport.mock.calls[0][0]).toBe('GET');
    expect(transport.mock.calls[0][1].pathname).toBe(
      '/api/v1/platform/operations/projection',
    );
    const executive = await client.executive(incoming(), actor, {
      from: '2026-06-01',
      to: '2026-06-30',
    });
    expect(transport.mock.calls[1][1].searchParams.get('from')).toBe('2026-06-01');
    expect(executive.metrics.map((metric) => metric.total)).toEqual(['1', '0']);
    const sources = await client.executiveSources(incoming(), actor, {
      metric: 'failed-provisioning',
      group: '2026-06',
      from: '2026-06-01',
      to: '2026-06-30',
      page: 2,
    });
    expect(transport.mock.calls[2][1].searchParams.get('page')).toBe('2');
    expect(sources.rows).toEqual([{ created_at: 1, action: 'failed' }]);
    const invalid = createNativePlatformAdminClient(
      JSON.parse(config()),
      vi.fn().mockResolvedValue(
        ok({
          data: {
            ...executiveSnapshotData,
            metrics: [{ ...executiveSnapshotData.metrics[0], total: 1 }, executiveSnapshotData.metrics[1]],
          },
          context,
        }),
      ) as never,
    );
    await expect(
      invalid.executive(incoming(), actor, { from: '2026-06-01', to: '2026-06-30' }),
    ).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID' });
    await expect(
      client.executive(incoming(), actor, {
        metric: 'failed-provisioning',
        group: '2026-06',
        from: '2026-06-01',
        to: '2026-06-30',
        page: 0,
      }),
    ).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_ADMIN_REQUEST_INVALID' });
  });

  it('maps configuration, context and response failures without inventing success', () => {
    expect(
      nativePlatformAdminFailure(new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_CONFIGURATION_INVALID')),
    ).toMatchObject({ status: 503, code: 'NATIVE_PLATFORM_ADMIN_UNAVAILABLE' });
    expect(
      nativePlatformAdminFailure(new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_CONTEXT_CHANGED')),
    ).toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
    expect(
      nativePlatformAdminFailure(new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_RESPONSE_INVALID')),
    ).toMatchObject({ status: 502 });
    expect(
      nativePlatformAdminFailure(new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_REQUEST_INVALID')),
    ).toMatchObject({ status: 422 });
    expect(nativePlatformAdminFailure(new Error('plain'))).toBeUndefined();
  });
});

const executiveWork = {
  userId: 'operator',
  holdingId: 'h003',
  holdingSlug: 'erpx',
  companyId: 'erpx',
  branchId: null,
  departmentId: null,
  assignmentId: 'work-assignment',
  role: 'platform_admin',
  assignmentScope: 'holding',
  scope: { kind: 'holding' } as const,
  authority: { revision: 7, assignmentVersion: 3 },
  modules: ['manage_erpx'],
  iam: { permissions: ['manage_erpx.access'] },
};
const executiveActor: NativePlatformExecutiveActor = {
  userId: 'operator',
  holdingId: 'h003',
  holdingSlug: 'erpx',
  companyId: 'erpx',
  branchId: null,
  departmentId: null,
  workAssignmentId: 'work-assignment',
  platformAssignmentId: 'platform-assignment',
  platformRole: 'platform_admin',
  authority: { revision: 7, assignmentVersion: 3 },
  modules: ['manage_erpx'],
  permissions: ['manage_erpx.access'],
};

describe('native Platform Executive transport', () => {
  it('rejects reserved, non-loopback, duplicate and insecure targets', () => {
    for (const privateBaseUrl of [
      'http://127.0.0.1:3101',
      'http://127.0.0.1:3100',
      'http://192.168.1.1:44991',
      'http://127.0.0.1:44991/',
    ])
      expect(() => nativePlatformExecutiveConfiguration(config({ privateBaseUrl }))).toThrow(
        NativePlatformExecutiveError,
      );
    expect(() =>
      nativePlatformExecutiveConfiguration(
        config({
          holdings: [
            { origin: 'https://erpx-internal.example.test', holdingSlug: 'erpx', holdingId: 'h003' },
            { origin: 'https://erpx-internal.example.test', holdingSlug: 'erpx', holdingId: 'h003' },
          ],
        }),
      ),
    ).toThrow(NativePlatformExecutiveError);
    expect(() =>
      nativePlatformExecutiveConfiguration(
        config({
          holdings: [
            { origin: 'http://erpx-internal.example.test', holdingSlug: 'erpx', holdingId: 'h003' },
          ],
        }),
      ),
    ).toThrow(NativePlatformExecutiveError);
  });

  it('forwards the exact host, cookie, platform assignment and snapshot query', async () => {
    const transport = vi.fn().mockResolvedValue(ok({ data: executiveSnapshotData, context }));
    const client = createNativePlatformExecutiveClient(JSON.parse(config()), transport as never);
    const result = await client.snapshot(incoming(), executiveActor, {
      from: '2026-06-01',
      to: '2026-06-30',
    });
    expect(result.metrics.map((metric) => metric.total)).toEqual(['1', '0']);
    const [method, url, headers] = transport.mock.calls[0]!;
    expect(method).toBe('GET');
    expect(url.pathname).toBe('/api/v1/platform/executive');
    expect(url.searchParams.get('from')).toBe('2026-06-01');
    expect(url.searchParams.get('to')).toBe('2026-06-30');
    expect(headers.get('host')).toBe('erpx-internal.example.test');
    expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
    expect(headers.get('x-erpx-platform-assignment')).toBe('platform-assignment');
    expect(headers.get('x-erpx-workspace-branch')).toBeNull();
  });

  it('forwards workspace selectors and bounded source fields', async () => {
    const branchContext = { ...context, branchId: 'branch-1' };
    const transport = vi.fn().mockResolvedValue(ok({ data: executiveSourcesData, context: branchContext }));
    const client = createNativePlatformExecutiveClient(JSON.parse(config()), transport as never);
    const branchActor: NativePlatformExecutiveActor = {
      ...executiveActor,
      branchId: 'branch-1',
      branchSlug: 'north',
    };
    const result = await client.sources(incoming(), branchActor, {
      metric: 'failed-provisioning',
      group: '2026-06',
      from: '2026-06-01',
      to: '2026-06-30',
      page: 2,
    });
    expect(result.rows).toEqual([{ created_at: 1, action: 'failed' }]);
    const [, url, headers] = transport.mock.calls[0]!;
    expect(url.pathname).toBe('/api/v1/platform/executive');
    expect(url.searchParams.get('metric')).toBe('failed-provisioning');
    expect(url.searchParams.get('group')).toBe('2026-06');
    expect(url.searchParams.get('page')).toBe('2');
    expect(headers.get('x-erpx-workspace-branch')).toBe('north');
    await expect(
      client.sources(incoming(), executiveActor, {
        metric: 'failed-provisioning',
        group: '2026-06',
        from: '2026-06-01',
        to: '2026-06-30',
        page: 0,
      }),
    ).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_EXECUTIVE_REQUEST_INVALID' });
  });

  it('rejects a foreign host, foreign context and malformed metrics', async () => {
    const client = createNativePlatformExecutiveClient(
      JSON.parse(config()),
      vi.fn().mockResolvedValue(ok({ data: executiveSnapshotData, context })) as never,
    );
    await expect(
      client.snapshot(
        incoming({ host: 'other.example.test' }),
        executiveActor,
        { from: '2026-06-01', to: '2026-06-30' },
      ),
    ).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_EXECUTIVE_AUTHORITY_INVALID' });
    await expect(
      client.snapshot(incoming(), { ...executiveActor, platformAssignmentId: 'other' }, {
        from: '2026-06-01',
        to: '2026-06-30',
      }),
    ).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_EXECUTIVE_RESPONSE_INVALID' });
    const malformed = createNativePlatformExecutiveClient(
      JSON.parse(config()),
      vi.fn().mockResolvedValue(
        ok({
          data: {
            ...executiveSnapshotData,
            metrics: [
              { ...executiveSnapshotData.metrics[0], total: 1 },
              executiveSnapshotData.metrics[1],
            ],
          },
          context,
        }),
      ) as never,
    );
    await expect(
      malformed.snapshot(incoming(), executiveActor, {
        from: '2026-06-01',
        to: '2026-06-30',
      }),
    ).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_EXECUTIVE_RESPONSE_INVALID' });
  });

  it('keeps the retained read only while unconfigured and surfaces configured failures', async () => {
    let retained = 0;
    expect(
      await nativePlatformExecutiveOrRetained(
        async () => undefined,
        async () => {
          retained += 1;
          return 'retained';
        },
      ),
    ).toBe('retained');
    expect(retained).toBe(1);
    await expect(
      nativePlatformExecutiveOrRetained(
        async () => {
          throw new NativePlatformExecutiveError('NATIVE_PLATFORM_EXECUTIVE_UNAVAILABLE');
        },
        async () => 'retained',
      ),
    ).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_EXECUTIVE_UNAVAILABLE' });
    const previous = process.env.ERPX_NATIVE_PLATFORM_EXECUTIVE_CONFIG;
    process.env.ERPX_NATIVE_PLATFORM_EXECUTIVE_CONFIG = config();
    try {
      await expect(
        nativePlatformExecutiveSnapshot(
          { ...executiveWork, holdingId: 'h001' },
          platform,
          incoming(),
        ),
      ).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_EXECUTIVE_CONTEXT_CHANGED' });
      await expect(
        nativePlatformExecutiveSnapshot(
          executiveWork,
          { ...platform, role: 'legal_editor' },
          incoming(),
        ),
      ).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_EXECUTIVE_CONTEXT_CHANGED' });
    } finally {
      if (previous === undefined) delete process.env.ERPX_NATIVE_PLATFORM_EXECUTIVE_CONFIG;
      else process.env.ERPX_NATIVE_PLATFORM_EXECUTIVE_CONFIG = previous;
    }
  });

  it('maps configuration, context, response and owner failures', () => {
    expect(
      nativePlatformExecutiveFailure(
        new NativePlatformExecutiveError('NATIVE_PLATFORM_EXECUTIVE_CONFIGURATION_INVALID'),
      ),
    ).toMatchObject({ status: 503, code: 'NATIVE_PLATFORM_EXECUTIVE_UNAVAILABLE' });
    expect(
      nativePlatformExecutiveFailure(
        new NativePlatformExecutiveError('NATIVE_PLATFORM_EXECUTIVE_CONTEXT_CHANGED'),
      ),
    ).toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
    expect(
      nativePlatformExecutiveFailure(
        new NativePlatformExecutiveError('NATIVE_PLATFORM_EXECUTIVE_RESPONSE_INVALID'),
      ),
    ).toMatchObject({ status: 502 });
    expect(
      nativePlatformExecutiveFailure(
        new NativePlatformExecutiveError('NATIVE_PLATFORM_EXECUTIVE_REQUEST_INVALID'),
      ),
    ).toMatchObject({ status: 422 });
    expect(
      nativePlatformExecutiveFailure(
        new NativePlatformExecutiveError('PLATFORM_EXECUTIVE_FORBIDDEN', 403, 'Denied.'),
      ),
    ).toMatchObject({ status: 403, code: 'PLATFORM_EXECUTIVE_FORBIDDEN' });
    expect(nativePlatformExecutiveFailure(new Error('plain'))).toBeUndefined();
  });
});
