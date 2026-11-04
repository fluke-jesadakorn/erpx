import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  createNativePlatformAdminClient,
  nativePlatformAdminConfiguration,
  nativePlatformAdminFailure,
  NativePlatformAdminError,
  type NativePlatformAdminActor,
} from '../../../packages/feature-platform_administration/src/adapters/server/native-platform-administration';
import { platformAdminActor } from '../../../packages/feature-platform_administration/src/lib/native-platform-administration';

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
