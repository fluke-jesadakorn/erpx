import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { OrganizationAccessData } from '@erpx/feature-organization/lib/organization-access';
import type { AdminNativeOrganizationWorkspace } from '@erpx/workplace/lib/native-admin-organization';
import { ApiError } from '@erpx/server-runtime/public/http';

vi.mock('server-only', () => ({}));

const native = vi.hoisted(() => ({
  workspace: vi.fn(),
  failure: vi.fn(),
}));

vi.mock('@erpx/feature-identity_access/lib/permissions', () => ({
  hasPermission: (actor: { iam?: { permissions?: readonly string[] } }, permission: string) =>
    actor.iam?.permissions?.includes(permission) ?? false,
}));

vi.mock('@erpx/feature-organization/lib/native-organization', () => {
  class NativeOrganizationError extends Error {
    constructor(
      public readonly code: string,
      public readonly status?: number,
    ) {
      super(code);
      this.name = 'NativeOrganizationError';
    }
  }
  return {
    nativeOrganizationWorkspace: native.workspace,
    nativeOrganizationFailure: native.failure,
    NativeOrganizationError,
    nativeOrganizationAccessPage: async (
      _actor: unknown,
      _slug: string,
      _incoming: unknown,
      retained: () => Promise<Record<string, unknown>>,
    ) => {
      let workspace: Record<string, unknown> | undefined;
      try {
        workspace = (await native.workspace()) as Record<string, unknown> | undefined;
      } catch (error) {
        const failure = (
          native.failure as (value: unknown) => { status: number; code: string; message: string } | undefined
        )(error);
        if (failure) throw Object.assign(new Error(failure.message), failure);
        throw error;
      }
      const projection = await retained();
      return workspace ? { ...projection, positions: workspace.positions } : projection;
    },
  };
});

import {
  composeAdminOrganizationAccess,
  nativeAdminOrganization,
  readAdminOrganization,
} from '@erpx/workplace/lib/native-admin-organization';

class NativeOrganizationError extends Error {
  constructor(public readonly code: string) {
    super(code);
    this.name = 'NativeOrganizationError';
  }
}

const company = 'company-1';
const branch = 'branch-1';
const department = 'department-1';

const actor = (permissions: string[] = ['people.manage']): WorkContext =>
  ({
    userId: 'user-1',
    holdingSlug: 'acme',
    companyId: company,
    branchId: branch,
    departmentId: department,
    role: 'holding_owner',
    scope: { kind: 'branch', holdingSlug: 'acme', holdingId: 'h001', holdingName: 'Acme', companyId: company, branchId: branch },
    assignmentScope: 'holding',
    modules: ['organization'],
    assignments: [],
    workspaces: [],
    iam: { permissions },
  }) as unknown as WorkContext;

const retained = (): OrganizationAccessData => ({
  positions: [
    {
      id: 'retained-position',
      company_id: company,
      branch_id: branch,
      department_id: department,
      title: 'Retained position',
      level_id: 'level-1',
      level_number: 1,
      reports_to_position_id: null,
      requirements: '',
      is_ceo: 0,
      roster_required: 1,
      active: 1,
      version: 1,
    },
  ],
  people: [
    {
      person_id: 'person-1',
      user_id: 'user-1',
      legal_name: 'Person One',
      email: 'private@example.test',
      engagement_id: 'engagement-1',
      engagement_version: 1,
      person_version: 1,
      company_id: company,
      branch_id: branch,
      department_id: department,
      position_id: 'retained-position',
      manager_person_id: null,
      level_id: 'level-1',
      level_number: 1,
      level_name: 'Level 1',
      status: 'active',
      person_status: 'active',
      start_date: '2026-01-01',
      end_date: null,
      job_title: 'Owner',
      issues: [],
    },
  ],
  nodes: [
    { id: company, kind: 'company', slug: 'company', display_name: 'Company', branch_id: null, status: 'active', hierarchy_revision: 2 },
  ],
  levels: [{ id: 'level-1', name: 'Level 1', level_number: 1 }],
  canManage: true,
  canMovePeople: true,
  actorId: 'user-1',
});

const position = (id: string, overrides: Record<string, unknown> = {}) => ({
  id,
  company_id: company,
  branch_id: branch,
  department_id: department,
  title: id,
  level_id: 'level-1',
  level_number: 1,
  reports_to_position_id: null,
  requirements: '',
  is_ceo: 0,
  roster_required: 1,
  active: 1,
  version: 1,
  ...overrides,
});

const workspace = (overrides: Partial<AdminNativeOrganizationWorkspace> = {}): AdminNativeOrganizationWorkspace => ({
  nodes: [
    { id: company, kind: 'company', slug: 'company', display_name: 'Company', parent_id: null, company_id: company, branch_id: null, status: 'active', hierarchy_revision: 2 },
    { id: branch, kind: 'branch', slug: 'branch', display_name: 'Branch', parent_id: company, company_id: company, branch_id: branch, status: 'active', hierarchy_revision: 2 },
  ],
  positions: [position('native-position')],
  levels: [{ id: 'level-1', name: 'Level 1', level_number: 1 }],
  actorId: 'user-1',
  canManage: true,
  ...overrides,
});

const adminActor = { userId: 'user-1', companyId: company, branchId: branch, departmentId: department };

beforeEach(() => {
  native.workspace.mockReset();
  native.failure.mockReset();
});

describe('composeAdminOrganizationAccess', () => {
  it('uses native Organization tree data and preserves retained People and manage ceilings', () => {
    const before = structuredClone(retained());
    const result = composeAdminOrganizationAccess(
      retained(),
      workspace({
        positions: [position('native-position'), position('other-branch', { branch_id: 'branch-2' })],
        levels: [{ id: 'level-2', name: 'Level 2', level_number: 2 }],
      }),
      adminActor,
    );
    expect(result.nodes.map((item) => item.id)).toEqual([company, branch]);
    expect(result.positions.map((item) => item.id)).toEqual(['native-position']);
    expect(result.levels).toEqual([{ id: 'level-2', name: 'Level 2', level_number: 2 }]);
    expect(result.people).toEqual(before.people);
    expect(result.canManage).toBe(true);
    expect(result.canMovePeople).toBe(true);
    expect(result.actorId).toBe('user-1');
    expect(retained()).toEqual(before);
  });

  it('drops inactive and out-of-scope native positions without filtering the authoritative People rows', () => {
    const result = composeAdminOrganizationAccess(
      retained(),
      workspace({
        positions: [
          position('active'),
          position('inactive', { active: 0 }),
          position('other-branch', { branch_id: 'branch-2' }),
          position('other-department', { department_id: 'department-2' }),
          position('no-branch', { branch_id: null }),
        ],
      }),
      adminActor,
    );
    expect(result.positions.map((item) => item.id)).toEqual(['active']);
    expect(result.people).toHaveLength(1);
  });

  it('never raises the retained Company-manage ceiling from native data', () => {
    const noManage = { ...retained(), canManage: false };
    expect(composeAdminOrganizationAccess(noManage, workspace({ canManage: true }), adminActor).canManage).toBe(false);
    expect(composeAdminOrganizationAccess(retained(), workspace({ canManage: false }), adminActor).canManage).toBe(false);
  });

  it('rejects a native actor or Company mismatch before accepting the read', () => {
    expect(() =>
      composeAdminOrganizationAccess(retained(), workspace({ actorId: 'other-user' }), adminActor),
    ).toThrow(expect.objectContaining({ status: 502, code: 'NATIVE_ORGANIZATION_RESPONSE_INVALID' }));
    expect(() =>
      composeAdminOrganizationAccess(
        retained(),
        workspace({
          nodes: [
            { id: company, kind: 'company', slug: 'company', display_name: 'Company', parent_id: null, company_id: 'other-company', branch_id: null, status: 'active', hierarchy_revision: 2 },
          ],
        }),
        adminActor,
      ),
    ).toThrow(expect.objectContaining({ status: 502, code: 'NATIVE_ORGANIZATION_RESPONSE_INVALID' }));
    expect(() =>
      composeAdminOrganizationAccess(
        retained(),
        workspace({ positions: [position('foreign', { company_id: 'other-company' })] }),
        adminActor,
      ),
    ).toThrow(expect.objectContaining({ status: 502, code: 'NATIVE_ORGANIZATION_RESPONSE_INVALID' }));
  });

  it('rejects duplicate native identities instead of presenting an ambiguous tree', () => {
    expect(() =>
      composeAdminOrganizationAccess(
        retained(),
        workspace({
          nodes: [
            { id: company, kind: 'company', slug: 'company', display_name: 'Company', parent_id: null, company_id: company, branch_id: null, status: 'active', hierarchy_revision: 2 },
            { id: company, kind: 'branch', slug: 'duplicate', display_name: 'Duplicate', parent_id: null, company_id: company, branch_id: null, status: 'active', hierarchy_revision: 2 },
          ],
        }),
        adminActor,
      ),
    ).toThrow(expect.objectContaining({ status: 502, code: 'NATIVE_ORGANIZATION_RESPONSE_INVALID' }));
    expect(() =>
      composeAdminOrganizationAccess(
        retained(),
        workspace({ positions: [position('duplicate'), position('duplicate')] }),
        adminActor,
      ),
    ).toThrow(expect.objectContaining({ status: 502, code: 'NATIVE_ORGANIZATION_RESPONSE_INVALID' }));
  });
});

describe('nativeAdminOrganization', () => {
  it('keeps the retained read when the actor lacks people.manage', async () => {
    await expect(
      nativeAdminOrganization(actor(['organization.manage']), 'acme', {}, new Headers()),
    ).resolves.toBeUndefined();
    expect(native.workspace).not.toHaveBeenCalled();
  });

  it('forwards the current actor, Holding and headers to the existing owner port', async () => {
    const result = workspace();
    native.workspace.mockResolvedValue(result);
    const incoming = new Headers({ cookie: 'erpx_session=current' });
    await expect(
      nativeAdminOrganization(actor(), 'acme', { q: 'finance' }, incoming),
    ).resolves.toBe(result);
    expect(native.workspace).toHaveBeenCalledWith(
      expect.objectContaining({ userId: 'user-1', holdingSlug: 'acme' }),
      'acme',
      { q: 'finance' },
      incoming,
    );
  });

  it('returns undefined only when the owner transport is not configured', async () => {
    native.workspace.mockResolvedValue(undefined);
    await expect(nativeAdminOrganization(actor(), 'acme', {}, new Headers())).resolves.toBeUndefined();
  });

  it('rejects a changed Holding context before calling the owner', async () => {
    await expect(
      nativeAdminOrganization(actor(), 'other', {}, new Headers()),
    ).rejects.toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
    expect(native.workspace).not.toHaveBeenCalled();
  });

  it('maps a configured owner failure and never falls back to the retained read', async () => {
    native.workspace.mockRejectedValue(new NativeOrganizationError('NATIVE_ORGANIZATION_CONFIGURATION_INVALID'));
    native.failure.mockReturnValue({
      status: 503,
      code: 'NATIVE_ORGANIZATION_UNAVAILABLE',
      message: 'The organization workspace is unavailable.',
    });
    await expect(nativeAdminOrganization(actor(), 'acme', {}, new Headers())).rejects.toEqual(
      expect.objectContaining({
        status: 503,
        code: 'NATIVE_ORGANIZATION_UNAVAILABLE',
        message: 'The organization workspace is unavailable.',
      }),
    );
    expect(native.failure).toHaveBeenCalledTimes(1);
  });

  it('propagates an unexpected owner error unchanged', async () => {
    const failure = new Error('unexpected owner failure');
    native.workspace.mockRejectedValue(failure);
    native.failure.mockReturnValue(undefined);
    await expect(nativeAdminOrganization(actor(), 'acme', {}, new Headers())).rejects.toBe(failure);
  });

  it('exposes the bounded API error type for page and API callers', () => {
    expect(new ApiError(502, 'NATIVE_ORGANIZATION_RESPONSE_INVALID', 'invalid')).toMatchObject({
      status: 502,
      code: 'NATIVE_ORGANIZATION_RESPONSE_INVALID',
    });
  });
});

describe('readAdminOrganization', () => {
  it('keeps the exact retained projection when no owner transport is configured', async () => {
    native.workspace.mockResolvedValue(undefined);
    const retainedProjection = retained();
    await expect(
      readAdminOrganization(retainedProjection, actor(), 'acme', {}, new Headers()),
    ).resolves.toBe(retainedProjection);
  });

  it('composes the same owner projection for the SSR page and the Admin JSON API', async () => {
    native.workspace.mockResolvedValue(workspace({ positions: [position('api-position')] }));
    const result = await readAdminOrganization(
      Promise.resolve(retained()),
      actor(),
      'acme',
      {},
      new Headers(),
    );
    expect(result.positions.map((item) => item.id)).toEqual(['api-position']);
    expect(result.people).toEqual(retained().people);
    expect(result.actorId).toBe('user-1');
  });

  it('surfaces a configured owner failure before returning any retained projection', async () => {
    native.workspace.mockRejectedValue(new NativeOrganizationError('NATIVE_ORGANIZATION_UNAVAILABLE'));
    native.failure.mockReturnValue({
      status: 503,
      code: 'NATIVE_ORGANIZATION_UNAVAILABLE',
      message: 'The organization workspace is unavailable.',
    });
    await expect(
      readAdminOrganization(retained(), actor(), 'acme', {}, new Headers()),
    ).rejects.toMatchObject({ status: 503, code: 'NATIVE_ORGANIZATION_UNAVAILABLE' });
  });
});
