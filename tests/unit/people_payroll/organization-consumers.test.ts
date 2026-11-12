import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { OrganizationAccessData } from '@erpx/feature-organization/lib/organization-access';
import { NativeOrganizationError } from '../../../packages/feature-organization/src/adapters/server/native-organization';
import {
  organizationAccessNativeFirst,
  type PeopleOrganizationAccessDependencies,
} from '../../../packages/server-people_payroll/src/adapters/native-organization-access';
import { lifecyclePositionMoveDrain } from '../../../packages/server-people_payroll/src/adapters/organization-lifecycle-drain';

const headerState = vi.hoisted(() => {
  const holder = { current: new Headers() };
  return { holder, headers: vi.fn(async () => holder.current) };
});
vi.mock('next/headers', () => ({ headers: headerState.headers }));

const actor = {
  userId: 'user',
  holdingId: 'h001',
  holdingSlug: 'acme',
  companyId: 'company',
  branchId: null,
  departmentId: null,
  assignmentId: 'assignment',
  role: 'custom_organization',
  assignmentScope: 'holding',
  scope: { kind: 'holding' as const },
  authority: { revision: 7, assignmentVersion: 3 },
  modules: ['people'],
  iam: { permissions: ['people.manage', 'organization.manage'] },
} as unknown as WorkContext;

const retainedAccess = (): OrganizationAccessData => ({
  positions: [],
  people: [],
  nodes: [],
  levels: [],
  canManage: true,
  canMovePeople: true,
  actorId: 'user',
});

const nativeAccess: OrganizationAccessData = {
  ...retainedAccess(),
  positions: [
    {
      id: 'position-native',
      company_id: 'company',
      branch_id: null,
      department_id: null,
      title: 'Native read position',
      level_id: 'level-1',
      level_number: 1,
      reports_to_position_id: null,
      is_ceo: 0,
      roster_required: 0,
      active: 1,
      version: 1,
    },
  ],
};

const incoming = new Headers({ host: 'acme.example.test', cookie: 'session=value' });
const request = async () => incoming as Pick<Headers, 'get'>;

const configured = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:3199',
  timeoutMs: 5000,
  holdings: [{ origin: 'https://acme.example.test', holdingSlug: 'acme', holdingId: 'h001' }],
});

const originalConfiguration = process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
afterEach(() => {
  if (originalConfiguration === undefined) delete process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
  else process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = originalConfiguration;
});

describe('People Payroll native Organization access consumer', () => {
  it('keeps the retained access read and does no request/native work when unconfigured', async () => {
    delete process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
    const retained = vi.fn(async () => retainedAccess());
    const page: NonNullable<PeopleOrganizationAccessDependencies['page']> = vi.fn(async () => nativeAccess);

    await expect(
      organizationAccessNativeFirst(actor, retained, { request, page }),
    ).resolves.toEqual(retainedAccess());

    expect(retained).toHaveBeenCalledOnce();
    expect(page).not.toHaveBeenCalled();
    expect(headerState.headers).not.toHaveBeenCalled();
  });

  it('selects the native access read over the retained read when configured', async () => {
    process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = configured;
    const retained = vi.fn(async () => retainedAccess());
    const page: NonNullable<PeopleOrganizationAccessDependencies['page']> = vi.fn(async () => nativeAccess);

    await expect(
      organizationAccessNativeFirst(actor, retained, { request, page }),
    ).resolves.toBe(nativeAccess);

    expect(page).toHaveBeenCalledWith(actor, 'acme', incoming, retained);
    expect(retained).not.toHaveBeenCalled();
  });

  it('maps a configured native failure instead of falling back to the retained read', async () => {
    process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = configured;
    const retained = vi.fn(async () => retainedAccess());
    const page: NonNullable<PeopleOrganizationAccessDependencies['page']> = vi.fn(async () => {
      throw new NativeOrganizationError('NATIVE_ORGANIZATION_UNAVAILABLE', 503);
    });

    await expect(
      organizationAccessNativeFirst(actor, retained, { request, page }),
    ).rejects.toMatchObject({ status: 503, code: 'NATIVE_ORGANIZATION_UNAVAILABLE' });

    expect(retained).not.toHaveBeenCalled();
  });

  it('fails closed when the configured native request scope is unavailable', async () => {
    process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = configured;
    const retained = vi.fn(async () => retainedAccess());
    const page: NonNullable<PeopleOrganizationAccessDependencies['page']> = vi.fn(async () => nativeAccess);
    const unavailable = async (): Promise<Pick<Headers, 'get'>> => {
      throw new Error('headers() outside a request scope');
    };

    await expect(
      organizationAccessNativeFirst(actor, retained, { request: unavailable, page }),
    ).rejects.toMatchObject({ status: 503, code: 'NATIVE_ORGANIZATION_UNAVAILABLE' });

    expect(page).not.toHaveBeenCalled();
    expect(retained).not.toHaveBeenCalled();
  });

  it('fails closed on a malformed configured transport before the retained read', async () => {
    process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = '{';
    const retained = vi.fn(async () => retainedAccess());

    await expect(
      organizationAccessNativeFirst(actor, retained, { request }),
    ).rejects.toBeInstanceOf(ApiError);

    expect(retained).not.toHaveBeenCalled();
  });
});

describe('People Payroll lifecycle position-move drain selector', () => {
  const database = {};
  const retainedDrain = () => vi.fn(async (_database: unknown, now: number) => now + 1);

  it('keeps the retained position-move drain when the native Organization is unconfigured', async () => {
    const retained = retainedDrain();

    await expect(lifecyclePositionMoveDrain(database, retained, {})(1_000)).resolves.toBe(1_001);

    expect(retained).toHaveBeenCalledWith(database, 1_000);
  });

  it('skips the retained drain without invoking it when the native Organization is configured', async () => {
    const retained = retainedDrain();

    await expect(
      lifecyclePositionMoveDrain(database, retained, { ERPX_NATIVE_ORGANIZATION_CONFIG: configured })(2_000),
    ).resolves.toBe(0);

    expect(retained).not.toHaveBeenCalled();
  });

  it('treats a present but empty configuration as configured and skips the retained drain', async () => {
    const retained = retainedDrain();

    await expect(
      lifecyclePositionMoveDrain(database, retained, { ERPX_NATIVE_ORGANIZATION_CONFIG: '' })(3_000),
    ).resolves.toBe(0);

    expect(retained).not.toHaveBeenCalled();
  });

  it('uses the process environment when no explicit environment is supplied', async () => {
    const retained = retainedDrain();
    process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = configured;

    await expect(lifecyclePositionMoveDrain(database, retained)(4_000)).resolves.toBe(0);

    expect(retained).not.toHaveBeenCalled();
  });

  it('drains through the retained port when the process environment is unset', async () => {
    delete process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
    const retained = retainedDrain();

    await expect(lifecyclePositionMoveDrain(database, retained)(5_000)).resolves.toBe(5_001);

    expect(retained).toHaveBeenCalledWith(database, 5_000);
  });
});
