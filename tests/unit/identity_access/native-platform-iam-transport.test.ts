import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  createNativePlatformIamClient,
  nativePlatformIamConfiguration,
  nativePlatformIamFailure,
  NativePlatformIamError,
  type NativePlatformIamActor,
} from '../../../packages/feature-platform_administration/src/adapters/server/native-platform-iam';
import {
  nativePlatformIamActor,
  nativePlatformIamAvailable,
  nativePlatformIamCommand,
} from '../../../packages/feature-platform_administration/src/lib/native-platform-administration';

const origin = 'https://erpx-internal.example.test';
const config = () =>
  JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44991',
    timeoutMs: 1000,
    holdings: [{ origin, holdingSlug: 'erpx', holdingId: 'h003' }],
  });
const incoming = (overrides: Record<string, string> = {}) =>
  new Headers({
    host: 'erpx-internal.example.test',
    cookie: '__Host-erpx-product=session',
    origin,
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
const platform = {
  userId: 'operator',
  assignmentId: 'platform-assignment',
  role: 'platform_admin',
};
const actor: NativePlatformIamActor = nativePlatformIamActor(work, platform);
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
  expiresAt: 1_000_000,
};

function server(data: unknown, body: string) {
  return new Response(JSON.stringify({ data, context: JSON.parse(body) }), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });
}

describe('native Platform IAM transport', () => {
  it('forwards the exact role command with Platform/work selectors and Origin', async () => {
    const transport = vi.fn(async (_method, url: URL, headers: Headers, body?: string) => {
      expect(url.href).toBe('http://127.0.0.1:44991/api/v1/platform/iam/roles');
      expect(headers.get('host')).toBe('erpx-internal.example.test');
      expect(headers.get('origin')).toBe(origin);
      expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
      expect(headers.get('x-erpx-platform-assignment')).toBe('platform-assignment');
      expect(headers.get('x-erpx-context-company')).toBe('erpx');
      expect(headers.get('x-erpx-context-assignment')).toBe('work-assignment');
      expect(headers.get('x-erpx-context-revision')).toBe('7');
      expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
      expect(headers.get('idempotency-key')).toBe('platform-role-key-1');
      expect(headers.get('x-erpx-method')).toBe('POST');
      expect(body).toBe('{"name":"Platform finance reader"}');
      return server({ id: 'platform-review-1', status: 'pending', roleId: 'platform_custom_1' }, JSON.stringify(context));
    });
    const client = createNativePlatformIamClient(JSON.parse(config()), transport as never);
    const data = await client.command(
      incoming(),
      actor,
      { kind: 'role-save' },
      '{"name":"Platform finance reader"}',
      'platform-role-key-1',
    );
    expect(data.id).toBe('platform-review-1');
    expect(transport).toHaveBeenCalledTimes(1);
  });

  it('maps the five command paths and rejects malformed identifiers', async () => {
    const transport = vi.fn(async (_method, url: URL) => {
      return server(
        { id: 'record-1' },
        JSON.stringify(context),
      );
    });
    const client = createNativePlatformIamClient(JSON.parse(config()), transport as never);
    for (const [command, suffix] of [
      [{ kind: 'review-decide', reviewId: 'review-1' }, '/reviews/review-1/decide'],
      [{ kind: 'access-review-start' }, '/access-reviews'],
      [
        { kind: 'access-review-item-decide', itemId: 'item-1' },
        '/access-review-items/item-1/decide',
      ],
      [
        { kind: 'access-review-cancel', campaignId: 'campaign-1' },
        '/access-reviews/campaign-1/cancel',
      ],
    ] as const) {
      await client.command(incoming(), actor, command, '{"reason":"Reviewed"}', 'platform-key-1');
      const call = transport.mock.calls.at(-1)!;
      expect((call[1] as URL).pathname).toBe(`/api/v1/platform/iam${suffix}`);
    }
    await expect(
      client.command(
        incoming(),
        actor,
        { kind: 'review-decide', reviewId: 'review/1' },
        '{}',
        'platform-key-1',
      ),
    ).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_IAM_REQUEST_INVALID' });
  });

  it('fails closed on host/origin/context drift and owner failures', async () => {
    const client = createNativePlatformIamClient(
      JSON.parse(config()),
      vi.fn(async () =>
        new Response(JSON.stringify({ error: { code: 'PLATFORM_OPERATIONS_FORBIDDEN', message: 'A current Platform assignment is required.' } }), {
          status: 403,
          headers: { 'content-type': 'application/json' },
        }),
      ) as never,
    );
    await expect(
      client.command(
        incoming({ origin: 'https://other.example.test' }),
        actor,
        { kind: 'role-save' },
        '{}',
        'platform-role-key-1',
      ),
    ).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_IAM_AUTHORITY_INVALID' });
    await expect(
      client.command(
        incoming({ host: 'other.example.test' }),
        actor,
        { kind: 'role-save' },
        '{}',
        'platform-role-key-1',
      ),
    ).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_IAM_AUTHORITY_INVALID' });

    const changed = createNativePlatformIamClient(
      JSON.parse(config()),
      vi.fn(async () => server({ id: 'record-1' }, JSON.stringify(context))) as never,
    );
    await expect(
      changed.command(
        incoming(),
        { ...actor, platformAssignmentId: 'other-assignment' },
        { kind: 'role-save' },
        '{}',
        'platform-role-key-1',
      ),
    ).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_IAM_CONTEXT_CHANGED' });

    const failure = nativePlatformIamFailure(
      new NativePlatformIamError('PLATFORM_OPERATIONS_FORBIDDEN', 403, 'No Platform assignment.'),
    );
    expect(failure).toEqual({
      status: 403,
      code: 'PLATFORM_OPERATIONS_FORBIDDEN',
      message: 'No Platform assignment.',
    });

    const rejected = createNativePlatformIamClient(
      JSON.parse(config()),
      vi.fn(async () =>
        new Response(
          JSON.stringify({ error: { code: 'ROLE_CHANGED', message: 'The role changed.' } }),
          { status: 409, headers: { 'content-type': 'application/json' } },
        ),
      ) as never,
    );
    await expect(
      rejected.command(incoming(), actor, { kind: 'role-save' }, '{}', 'platform-role-key-1'),
    ).rejects.toMatchObject({ code: 'ROLE_CHANGED', status: 409 });
  });

  it('keeps the retained handler while the Platform IAM configuration is unset', async () => {
    const previous = process.env.ERPX_NATIVE_PLATFORM_IAM_CONFIG;
    try {
      delete process.env.ERPX_NATIVE_PLATFORM_IAM_CONFIG;
      expect(nativePlatformIamAvailable()).toBe(false);
      expect(
        await nativePlatformIamCommand(
          work,
          platform,
          incoming(),
          { kind: 'role-save' },
          '{}',
          'platform-role-key-1',
        ),
      ).toBeUndefined();
      process.env.ERPX_NATIVE_PLATFORM_IAM_CONFIG = '{}';
      expect(nativePlatformIamAvailable()).toBe(true);
      await expect(
        nativePlatformIamCommand(
          work,
          platform,
          incoming(),
          { kind: 'role-save' },
          '{}',
          'platform-role-key-1',
        ),
      ).rejects.toMatchObject({ code: 'NATIVE_PLATFORM_IAM_CONFIGURATION_INVALID' });
    } finally {
      if (previous === undefined) delete process.env.ERPX_NATIVE_PLATFORM_IAM_CONFIG;
      else process.env.ERPX_NATIVE_PLATFORM_IAM_CONFIG = previous;
    }
    expect(() => nativePlatformIamConfiguration('{}')).toThrow(NativePlatformIamError);
  });
});
